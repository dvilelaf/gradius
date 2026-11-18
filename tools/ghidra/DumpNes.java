// Export NES program analysis: functions, symbols, xrefs, code/data map, pointer tables
import java.io.*;
import java.util.*;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.scalar.Scalar;
import ghidra.program.model.symbol.*;
import ghidra.app.decompiler.*;

public class DumpNes extends GhidraScript {
    private static final long[] TARGETS = { 0x8D92L, 0x9193L, 0xA5FCL, 0xA65CL, 0xA7CAL, 0x8B15L };
    private StringBuilder funs = new StringBuilder();
    private StringBuilder syms = new StringBuilder();
    private StringBuilder refs = new StringBuilder();
    private StringBuilder map = new StringBuilder();
    private StringBuilder dis = new StringBuilder();
    private StringBuilder dec = new StringBuilder();
    private AddressSpace romSpace;

    private Address rom(long v) {
        return romSpace == null ? toAddr(v) : romSpace.getAddress(v);
    }

    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : ".";
        Program p = currentProgram;
        println("Program: " + p.getName() + " lang=" + p.getLanguage().getLanguageID());

        for (MemoryBlock b : p.getMemory().getBlocks()) {
            if (b.isInitialized() && b.isExecute() && b.getStart().getOffset() == 0x8000) {
                romSpace = b.getStart().getAddressSpace();
            }
        }

        // ---------- add data refs for absolute operands (6502 loader creates none) ----------
        Listing lst = p.getListing();
        ReferenceManager rm = p.getReferenceManager();
        int addedRefs = 0;
        for (Instruction ins : lst.getInstructions(true)) {
            for (int i = 0; i < ins.getNumOperands(); i++) {
                for (Object o : ins.getOpObjects(i)) {
                    if (!(o instanceof Scalar)) continue;
                    long v = ((Scalar) o).getUnsignedValue();
                    if (v < 0x10000) {
                        Address t = (v >= 0x8000 && romSpace != null) ? romSpace.getAddress(v) : toAddr(v);
                        if (p.getMemory().contains(t) && !hasRef(rm, ins.getAddress(), t)) {
                            rm.addMemoryReference(ins.getAddress(), t, RefType.DATA, SourceType.ANALYSIS, i);
                            addedRefs++;
                        }
                    }
                }
            }
        }
        println("added data refs: " + addedRefs);

        // ---------- memory blocks ----------
        map.append("# Memory blocks\n");
        for (MemoryBlock b : p.getMemory().getBlocks()) {
            map.append(String.format("%-14s %s-%s size=0x%X init=%b exec=%b write=%b vol=%b type=%s%n",
                b.getName(), b.getStart(), b.getEnd(), b.getSize(), b.isInitialized(),
                b.isExecute(), b.isWrite(), b.isVolatile(), b.getType()));
        }

        // ---------- symbols ----------
        syms.append("# addr\ttype\tnamespace\tname\n");
        SymbolTable st = p.getSymbolTable();
        int nsym = 0;
        for (Symbol s : st.getAllSymbols(true)) {
            syms.append(String.format("%s\t%s\t%s\t%s%n", s.getAddress(), s.getSymbolType(),
                s.getParentNamespace().getName(true), s.getName(true)));
            nsym++;
        }

        // ---------- functions ----------
        FunctionManager fm = p.getFunctionManager();
        List<Function> fl = new ArrayList<>();
        for (Function f : fm.getFunctions(true)) fl.add(f);
        funs.append("# entry\tsize_bytes\tinsns\tcallers\tname\n");
        for (Function f : fl) {
            int callers = 0;
            for (Reference r : rm.getReferencesTo(f.getEntryPoint())) {
                if (r.getReferenceType().isCall() || r.getReferenceType().isJump()) callers++;
            }
            funs.append(String.format("%s\t0x%X\t%d\t%d\t%s%n", f.getEntryPoint(),
                f.getBody().getNumAddresses(), countInsns(f.getBody()), callers, f.getName()));
        }

        // ---------- code / data / undefined ranges ----------
        AddressSet code = new AddressSet();
        AddressSet data = new AddressSet();
        AddressSet undef = new AddressSet();
        int ninsn = 0;
        Memory mem = p.getMemory();
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isInitialized()) continue;
            long n = b.getSize();
            for (long i = 0; i < n; i++) {
                Address a = b.getStart().add(i);
                Instruction ins = lst.getInstructionContaining(a);
                if (ins != null) { code.add(a); ninsn++; }
                else if (lst.getDataAt(a) != null) { data.add(a); }
                else { undef.add(a); }
            }
        }
        map.append(String.format("%n# Classification: instructions=%d, defined data bytes=%d, undefined bytes=%d%n",
            ninsn, data.getNumAddresses(), undef.getNumAddresses()));
        appendRanges(map, "CODE", code);
        appendRanges(map, "DATA", data);
        appendRanges(map, "UNDEF", undef);

        // ---------- operand use index ----------
        Map<Long, List<String>> opUses = new HashMap<>();
        for (Instruction ins : lst.getInstructions(true)) {
            for (int op = 0; op < ins.getNumOperands(); op++) {
                for (Object o : ins.getOpObjects(op)) {
                    if (o instanceof Scalar) {
                        long v = ((Scalar) o).getUnsignedValue();
                        opUses.computeIfAbsent(v, k -> new ArrayList<>())
                            .add(ins.getAddress() + ":" + ins.toString());
                    }
                }
            }
        }

        // ---------- pointer tables ----------
        // scan PRG for LE16 values that point into ROM; group contiguous runs
        List<long[]> tables = new ArrayList<>();  // {base, count, firstTarget}
        List<long[]> ptrs = new ArrayList<>();    // {location, value}
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isInitialized() || !b.isExecute()) continue;
            long n = b.getSize();
            for (int parity = 0; parity < 2; parity++) {
                long runStart = -1, runCount = 0;
                for (long i = parity; i + 1 < n; i += 2) {
                    Address a = b.getStart().add(i);
                    int lo = mem.getByte(a) & 0xFF, hi = mem.getByte(a.add(1)) & 0xFF;
                    long v = lo | (hi << 8);
                    boolean inRom = v >= 0x8000 && v <= 0xFFFF;
                    if (inRom && lst.getInstructionContaining(a) == null) {
                        long off = a.getOffset();
                        ptrs.add(new long[] { off, v });
                        if (runStart < 0 || off != runStart + runCount * 2) {
                            if (runCount >= 4) tables.add(new long[] { runStart, runCount, 0 });
                            runStart = off; runCount = 1;
                        } else runCount++;
                    } else {
                        if (runCount >= 4) tables.add(new long[] { runStart, runCount, 0 });
                        runStart = -1; runCount = 0;
                    }
                }
                if (runCount >= 4) tables.add(new long[] { runStart, runCount, 0 });
            }
        }
        map.append("\n# Pointer tables referenced by an instruction operand (LE16 runs >= 4 into ROM)\n");
        Set<Long> seenBases = new HashSet<>();
        List<long[]> realTables = new ArrayList<>();
        for (long[] t : tables) {
            if (!opUses.containsKey(t[0]) || !seenBases.add(t[0])) continue;
            realTables.add(t);
        }
        for (long[] t : realTables) {
            long base = t[0], cnt = t[1];
            StringBuilder sb = new StringBuilder();
            int shown = 0;
            for (long i = 0; i < cnt && shown < 6; i++) {
                Address a = rom(base + i * 2);
                int lo = mem.getByte(a) & 0xFF, hi = mem.getByte(a.add(1)) & 0xFF;
                sb.append(String.format(" %04X", lo | (hi << 8)));
                shown++;
            }
            map.append(String.format("TABLE %04X count=%d first:%s%s  referenced_by=%s%n",
                base, cnt, sb, cnt > 6 ? " ..." : "", String.join(" | ", opUses.get(base))));
        }

        // ---------- refs to key addresses ----------
        refs.append("# Cross references and pointer occurrences for key addresses\n");
        for (long t : TARGETS) refs.append(targetReport(t, st, rm, lst, mem, ptrs, realTables, opUses));

        // ---------- disassembly ----------
        dis.append("# addr\tbytes\tinstruction\n");
        for (Instruction ins : lst.getInstructions(true)) {
            dis.append(String.format("%s\t%s\t%s%n", ins.getAddress(), bytes(ins), ins.toString()));
        }

        // ---------- decompiler probe ----------
        DecompInterface ifc = new DecompInterface();
        ifc.openProgram(p);
        dec.append("# decompiler probe\n");
        List<Function> sorted = new ArrayList<>(fl);
        sorted.sort((x, y) -> (int) (y.getBody().getNumAddresses() - x.getBody().getNumAddresses()));
        int tried = 0, ok = 0;
        for (Function f : sorted) {
            if (tried >= 10) break;
            tried++;
            try {
                DecompileResults r = ifc.decompileFunction(f, 60, monitor);
                if (r != null && r.decompileCompleted() && r.getDecompiledFunction() != null) {
                    ok++;
                    dec.append("\n// ").append(f.getEntryPoint()).append(" ").append(f.getName()).append("\n");
                    dec.append(r.getDecompiledFunction().getC());
                } else {
                    dec.append("\n// FAILED ").append(f.getEntryPoint()).append(": ")
                       .append(r == null ? "null" : r.getErrorMessage()).append("\n");
                }
            } catch (Exception e) {
                dec.append("\n// EXCEPTION ").append(f.getEntryPoint()).append(": ").append(e).append("\n");
            }
        }
        dec.append("\n// tried=" + tried + " ok=" + ok + "\n");

        write(outDir + "/functions.txt", funs.toString());
        write(outDir + "/symbols.txt", syms.toString());
        write(outDir + "/refs.txt", refs.toString());
        write(outDir + "/codemap.txt", map.toString());
        write(outDir + "/disasm.txt", dis.toString());
        write(outDir + "/decompiled.txt", dec.toString());
        println("functions=" + fl.size() + " symbols=" + nsym + " tables=" + realTables.size()
            + " wrote exports to " + outDir);
    }

    private String targetReport(long t, SymbolTable st, ReferenceManager rm, Listing lst,
            Memory mem, List<long[]> ptrs, List<long[]> tables, Map<Long, List<String>> opUses) {
        StringBuilder sb = new StringBuilder();
        Address a = rom(t);
        sb.append(String.format("%n## %04X%n", t));
        Instruction ins = lst.getInstructionAt(a);
        if (ins != null) {
            Function f = lst.getFunctionContaining(a);
            sb.append(String.format("CODE (inside %s)%n  %s  %s%n",
                f == null ? "no function" : f.getName(), a, ins));
        } else {
            Data d = lst.getDataAt(a);
            sb.append(d == null ? "UNDEFINED (no code unit)\n" : "DATA (" + d.getDataType().getName() + ")\n");
        }
        // containing table?
        for (long[] tb : tables) {
            long base = tb[0], len = tb[1] * 2;
            if (t >= base && t < base + len) sb.append(String.format("inside pointer table at %04X (+%d)%n", base, t - base));
        }
        // xrefs (after synthetic data refs)
        Set<String> instrRefs = new HashSet<>();
        int nx = 0;
        for (Reference r : rm.getReferencesTo(a)) {
            nx++;
            Function f = lst.getFunctionContaining(r.getFromAddress());
            sb.append(String.format("XREF from=%s type=%s func=%s%n", r.getFromAddress(),
                r.getReferenceType(), f == null ? "-" : f.getName()));
            instrRefs.add(r.getFromAddress().toString());
        }
        sb.append("xref_count=").append(nx).append("\n");
        List<String> uses = opUses.get(t);
        if (uses != null) for (String u : uses) {
            if (!instrRefs.contains(u.substring(0, u.indexOf(':'))))
                sb.append("OPERAND ").append(u).append("\n");
        }
        // pointer occurrences (LE16 equal to target)
        int np = 0;
        for (long[] pr : ptrs) {
            if (pr[1] == t) {
                long loc = pr[0];
                String tb = "";
                for (long[] tt : tables) if (loc >= tt[0] && loc < tt[0] + tt[1] * 2) tb = String.format(" table@%04X", tt[0]);
                sb.append(String.format("PTR_AT %04X%s%n", loc, tb));
                np++;
            }
        }
        sb.append("ptr_count=").append(np).append("\n");
        // first bytes
        try {
            byte[] bb = new byte[8];
            for (int i = 0; i < 8; i++) bb[i] = mem.getByte(a.add(i));
            sb.append("bytes: ");
            for (byte x : bb) sb.append(String.format("%02X ", x));
            sb.append("\n");
        } catch (Exception e) { }
        return sb.toString();
    }

    private boolean hasRef(ReferenceManager rm, Address from, Address to) {
        for (Reference r : rm.getReferencesFrom(from)) if (r.getToAddress().equals(to)) return true;
        return false;
    }

    private int countInsns(AddressSetView set) {
        int n = 0;
        for (Instruction i : currentProgram.getListing().getInstructions(set, true)) n++;
        return n;
    }

    private String bytes(Instruction ins) throws Exception {
        StringBuilder sb = new StringBuilder();
        for (byte b : ins.getBytes()) sb.append(String.format("%02X", b));
        return sb.toString();
    }

    private void appendRanges(StringBuilder sb, String tag, AddressSet set) {
        AddressRange cur = null;
        for (AddressRange r : set.getAddressRanges()) {
            if (cur == null) { cur = r; continue; }
            if (cur.getMaxAddress().add(1).equals(r.getMinAddress())) {
                cur = new AddressRangeImpl(cur.getMinAddress(), r.getMaxAddress());
            } else {
                emit(sb, tag, cur);
                cur = r;
            }
        }
        if (cur != null) emit(sb, tag, cur);
    }

    private void emit(StringBuilder sb, String tag, AddressRange r) {
        sb.append(String.format("%s\t%s-%s\t0x%X bytes%n", tag, r.getMinAddress(), r.getMaxAddress(), r.getLength()));
    }

    private void write(String path, String content) throws IOException {
        try (Writer w = new OutputStreamWriter(new FileOutputStream(path), "UTF-8")) {
            w.write(content);
        }
    }
}
