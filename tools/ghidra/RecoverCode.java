// Recover pointer-table-reached code that the direct call graph misses.
//
// The 6502 loader / auto-analysis leaves $8C7D-$EC17 as data because no JSR/JMP
// reaches most of it. Two things hide code there:
//
//  * inline dispatch: `JSR $83E9` (a pop-return-address dispatcher) is followed
//    by a 16-bit LE pointer table, and the helper jumps through one entry;
//  * named tables from earlier work ($8D91, $8E80, $A7CA/$A7CB, $A5F8).
//
// Every candidate entry is decoded by PseudoDisassembler before it is accepted.
// A target becomes a function only when the whole reachable path decodes, ends
// in RTS/RTI/JMP, and hits no BRK/undefined opcode. The guard matters: sprite
// and stage streams are full of 16-bit values that happen to look like code.
//
// Writes functions_full.txt (never functions.txt) and leaves all ROM bytes
// outside the repository.

import java.io.*;
import java.util.*;

import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.program.disassemble.Disassembler;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.plugin.core.analysis.AutoAnalysisManager;
import ghidra.app.script.GhidraScript;
import ghidra.app.util.PseudoDisassembler;
import ghidra.app.util.PseudoInstruction;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import ghidra.util.task.TaskMonitor;

public class RecoverCode extends GhidraScript {
    private static final byte[] INLINE_DISPATCH = { 0x20, (byte) 0xE9, (byte) 0x83 }; // JSR $83E9

    private Program p;
    private Memory mem;
    private Listing lst;
    private FunctionManager fm;
    private SymbolTable st;
    private AddressSpace romSpace;
    private PseudoDisassembler pd;
    private TaskMonitor mon;
    private AddressSet prgSet;

    // target -> evidence strings
    private final Map<Long, LinkedHashSet<String>> evidence = new LinkedHashMap<>();
    // table base -> display name
    private final Map<Long, String> tableNames = new LinkedHashMap<>();
    private final Set<Long> baselineFunctions = new LinkedHashSet<>();
    private final Set<Long> createdFunctions = new LinkedHashSet<>();
    private final Set<Long> pointerEntries = new LinkedHashSet<>();

    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : ".";
        p = currentProgram;
        mem = p.getMemory();
        lst = p.getListing();
        fm = p.getFunctionManager();
        st = p.getSymbolTable();
        mon = monitor;
        pd = new PseudoDisassembler(p);
        for (MemoryBlock b : mem.getBlocks()) {
            if (b.isInitialized() && b.isExecute() && b.getStart().getOffset() == 0x8000) {
                romSpace = b.getStart().getAddressSpace();
            }
        }
        if (romSpace == null) throw new IllegalStateException("no PRG block at $8000");
        prgSet = new AddressSet(rom(0x8000), rom(0xFFFF));

        for (Function f : fm.getFunctions(true)) baselineFunctions.add(f.getEntryPoint().getOffset());
        addDataRefs();

        // 1. Inline dispatch tables: `JSR $83E9` followed by LE16 pointers.
        int sites = 0;
        for (long addr = 0x8000L; addr + 2 < 0x10000L; addr++) {
            if (matches(addr, INLINE_DISPATCH)) {
                sites++;
                long site = addr;
                long base = site + 3;
                String name = String.format("PT_%04X_inline_%04X", base, site);
                int count = scanTable(base, name, String.format("inline JSR $83E9 @ %04X", site));
                clearTableBytes(base, count);
            }
        }

        // 2. Named tables from docs/analysis/ghidra.md. They hold sprite/stage
        //    streams, so most entries fail the decode check and create nothing.
        scanTable(0x8D91L, "PT_8D91_sprite_groups", "$8D91 sprite-group table");
        scanTable(0x8E80L, "PT_8E80_sprite_streams", "$8E80 sprite/entity stream table");
        scanTable(0x8E81L, "PT_8E81_sprite_streams", "$8E80 table, odd base");
        scanTable(0xA7CAL, "PT_A7CA_stage_sections", "$A7CA/$A7CB stage-section table");
        scanTable(0xA5F8L, "PT_A5F8_2word", "$A5F8 two-word table");

        // 3. Create functions for decoded pointer targets, then close over direct
        //    calls/tail jumps found inside the recovered bodies.
        Deque<Long> queue = new ArrayDeque<>();
        for (long t : new TreeSet<>(evidence.keySet())) queue.add(t);
        while (!queue.isEmpty()) {
            long t = queue.removeFirst();
            if (!isStartCandidate(t)) continue;
            List<Long> callees = createAt(t);
            for (long c : callees) if (isStartCandidate(c)) queue.addLast(c);
        }

        // Re-run auto-analysis so JSR targets inside recovered bodies become
        // functions before the export is written.
        AutoAnalysisManager am = AutoAnalysisManager.getAnalysisManager(p);
        am.startAnalysis(mon, false);
        am.waitForAnalysis(300, mon);

        // Labels make the recovered table bases visible next to the functions.
        for (Map.Entry<Long, String> e : tableNames.entrySet()) {
            Address a = rom(e.getKey());
            if (st.getPrimarySymbol(a) == null) {
                st.createLabel(a, e.getValue(), SourceType.ANALYSIS);
            }
        }

        export(outDir);
        println(String.format("recover: tables=%d inline_sites=%d pointer_entries=%d new_functions=%d total_functions=%d",
            tableNames.size(), sites, pointerEntries.size(), createdFunctions.size(), fm.getFunctionCount()));
    }

    /** Scan a LE16 table; stop at the first word that is not a decodable ROM target. */
    private int scanTable(long base, String name, String evidenceText) {
        int count = 0;
        while (base + count * 2 + 1 < 0x10000) {
            long v = word(base + count * 2);
            if (v < 0x8000 || v > 0xFFFF) break;
            if (!isDecodable(v)) break;
            LinkedHashSet<String> ev = evidence.computeIfAbsent(v, k -> new LinkedHashSet<>());
            ev.add(String.format("%s (+%d)", evidenceText, count * 2));
            pointerEntries.add(v);
            count++;
        }
        tableNames.put(base, name);
        return count;
    }

    /** The inline pointers are data: remove any stray decode that flowed over them. */
    private void clearTableBytes(long base, int count) {
        if (count <= 0) return;
        Address start = rom(base);
        Address end = rom(base + count * 2L - 1);
        AddressSet tableSet = new AddressSet(start, end);
        lst.clearCodeUnits(start, end, false);
        for (Function f : fm.getFunctions(true)) {
            if (!f.getBody().intersects(tableSet)) continue;
            AddressSet body = new AddressSet(f.getBody());
            body.delete(tableSet);
            try {
                if (body.isEmpty()) fm.removeFunction(f.getEntryPoint());
                else f.setBody(body);
            } catch (Exception e) {
                println(String.format("table trim failed for FUN at %04X: %s", f.getEntryPoint().getOffset(), e));
            }
        }
    }

    private List<Long> createAt(long target) {
        List<Long> callees = new ArrayList<>();
        Address entry = rom(target);
        if (lst.getFunctionContaining(entry) != null) return callees;
        Instruction existing = lst.getInstructionContaining(entry);
        if (existing != null && !existing.getAddress().equals(entry)) {
            // An orphan instruction (not owned by a function) has swallowed this
            // real entry. Split it only when no function owns the instruction.
            if (lst.getFunctionContaining(existing.getAddress()) != null) return callees;
            try {
                lst.clearCodeUnits(existing.getMinAddress(), existing.getMaxAddress(), false);
            } catch (Exception e) {
                return callees;
            }
            existing = lst.getInstructionContaining(entry);
            if (existing != null && !existing.getAddress().equals(entry)) return callees;
        }

        DisassembleCommand dc = new DisassembleCommand(entry, prgSet, true);
        dc.enableCodeAnalysis(false);
        dc.applyTo(p, mon);
        if (lst.getInstructionContaining(entry) == null) {
            // DisassembleCommand refuses some already-defined byte units; the
            // lower-level disassembler with an explicit PRG restriction does not.
            Disassembler dis = Disassembler.getDisassembler(p, mon, msg -> { });
            AddressSet out = dis.disassemble(new AddressSet(entry, entry), prgSet, null, true);
            if (out.isEmpty()) {
                println(String.format("disassemble failed at %04X", target));
                return callees;
            }
        }
        CreateFunctionCmd cf = new CreateFunctionCmd(String.format("FUN_PTR_%04X", target), entry,
            null, SourceType.ANALYSIS);
        if (!cf.applyTo(p, mon) || cf.getFunction() == null) {
            println(String.format("createFunction failed at %04X", target));
            return callees;
        }
        Function f = cf.getFunction();
        createdFunctions.add(target);
        String ev = String.join("; ", evidence.getOrDefault(target, new LinkedHashSet<>()));
        if (!ev.isEmpty()) f.setComment("Recovered from " + ev);

        for (Instruction ins : lst.getInstructions(f.getBody(), true)) {
            for (Reference r : ins.getReferencesFrom()) {
                if (!(r.getReferenceType().isCall() || r.getReferenceType().isJump())) continue;
                long to = r.getToAddress().getOffset();
                if (to >= 0x8000 && to <= 0xFFFF) callees.add(to);
            }
        }
        return callees;
    }

    private boolean isStartCandidate(long v) {
        if (v < 0x8000 || v > 0xFFFF) return false;
        Address a = rom(v);
        if (lst.getFunctionContaining(a) != null) return false;
        Instruction ins = lst.getInstructionContaining(a);
        if (ins != null && !ins.getAddress().equals(a)) {
            return lst.getFunctionContaining(ins.getAddress()) == null;
        }
        return true;
    }

    /** Full recursive 6502 decode: flow + branches must terminate cleanly. */
    private boolean isDecodable(long target) {
        if (target < 0x8000 || target > 0xFFFF) return false;
        Set<Long> seen = new HashSet<>();
        List<Long> work = new ArrayList<>();
        work.add(target);
        int n = 0;
        while (!work.isEmpty()) {
            long a = work.remove(work.size() - 1);
            if (a < 0x8000 || a > 0xFFFF || !seen.add(a)) continue;
            if (++n > 500) return false;
            PseudoInstruction ins;
            try {
                ins = pd.disassemble(rom(a));
            } catch (Exception e) {
                return false;
            }
            if (ins == null) return false;
            String m = ins.getMnemonicString().toUpperCase();
            if (m.equals("BRK") || m.equals("???") || m.equals("JAM") || m.equals("ILLEGAL")) return false;
            FlowType ft = ins.getFlowType();
            Address fall = ins.getFallThrough();
            if (ft.isTerminal()) {
                if (m.startsWith("RTS") || m.startsWith("RTI") || ft.isComputed() || ft.isJump() || ft.isCall()) return true;
                return false;
            }
            if (ft.isConditional()) {
                Address[] flows = ins.getFlows();
                if (flows != null) for (Address f : flows) if (f != null) work.add(f.getOffset());
                if (fall != null) work.add(fall.getOffset());
            } else if (ft.isCall()) {
                if (fall != null) work.add(fall.getOffset());
            } else if (ft.isJump()) {
                Address[] flows = ins.getFlows();
                if (flows != null) for (Address f : flows) if (f != null) work.add(f.getOffset());
            } else if (fall != null) {
                work.add(fall.getOffset());
            } else {
                return false;
            }
        }
        return false;
    }

    private void export(String outDir) throws IOException {
        List<Function> fs = new ArrayList<>();
        for (Function f : fm.getFunctions(true)) fs.add(f);
        fs.sort(Comparator.comparingLong(f -> f.getEntryPoint().getOffset()));

        StringBuilder sb = new StringBuilder();
        sb.append("# entry\tsize_bytes\tinsns\tcallers\tname\torigin\tevidence\n");
        int plain = 0, ptr = 0, call = 0;
        for (Function f : fs) {
            long e = f.getEntryPoint().getOffset();
            String origin;
            String ev = "-";
            if (baselineFunctions.contains(e)) {
                origin = "direct";
                plain++;
            } else if (evidence.containsKey(e)) {
                origin = "ptr";
                ev = String.join("; ", evidence.get(e));
                ptr++;
            } else {
                origin = "call";
                ev = "direct callee of recovered code";
                call++;
            }
            sb.append(String.format("%s\t0x%X\t%d\t%d\t%s\t%s\t%s%n",
                f.getEntryPoint(), f.getBody().getNumAddresses(), countInsns(f.getBody()),
                countCallers(f.getEntryPoint()), f.getName(), origin, ev));
        }
        write(outDir + "/functions_full.txt", sb.toString());

        StringBuilder tables = new StringBuilder();
        tables.append("# table\tsite/evidence\tentries\tdecoded\n");
        for (Map.Entry<Long, String> e : tableNames.entrySet()) {
            long base = e.getKey();
            int entries = 0, decoded = 0;
            while (base + entries * 2 + 1 < 0x10000) {
                long v = word(base + entries * 2);
                if (v < 0x8000 || v > 0xFFFF) break;
                entries++;
                if (evidence.containsKey(v)) decoded++;
            }
            tables.append(String.format("%04X\t%s\t%d\t%d%n", base, e.getValue(), entries, decoded));
        }
        write(outDir + "/pointer_tables.txt", tables.toString());
        println(String.format("functions_full: direct=%d ptr=%d call=%d total=%d", plain, ptr, call, fs.size()));
    }

    private int countInsns(AddressSetView set) {
        int n = 0;
        for (Instruction i : lst.getInstructions(set, true)) n++;
        return n;
    }

    private int countCallers(Address entry) {
        int n = 0;
        for (Reference r : p.getReferenceManager().getReferencesTo(entry)) {
            if (r.getReferenceType().isCall() || r.getReferenceType().isJump()) n++;
        }
        return n;
    }

    private void addDataRefs() {
        ReferenceManager rm = p.getReferenceManager();
        int added = 0;
        for (Instruction ins : lst.getInstructions(true)) {
            for (int i = 0; i < ins.getNumOperands(); i++) {
                for (Object o : ins.getOpObjects(i)) {
                    if (!(o instanceof ghidra.program.model.scalar.Scalar)) continue;
                    long v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                    if (v >= 0x10000) continue;
                    Address t = v >= 0x8000 ? rom(v) : toAddr(v);
                    if (p.getMemory().contains(t) && !hasRef(rm, ins.getAddress(), t)) {
                        rm.addMemoryReference(ins.getAddress(), t, RefType.DATA, SourceType.ANALYSIS, i);
                        added++;
                    }
                }
            }
        }
        println("added data refs: " + added);
    }

    private boolean hasRef(ReferenceManager rm, Address from, Address to) {
        for (Reference r : rm.getReferencesFrom(from)) if (r.getToAddress().equals(to)) return true;
        return false;
    }

    private boolean matches(long a, byte[] pat) {
        try {
            for (int i = 0; i < pat.length; i++) if (mem.getByte(rom(a + i)) != pat[i]) return false;
            return true;
        } catch (MemoryAccessException e) {
            return false;
        }
    }

    private long word(long a) {
        try {
            return (mem.getByte(rom(a)) & 0xFF) | ((mem.getByte(rom(a + 1)) & 0xFF) << 8);
        } catch (MemoryAccessException e) {
            return -1;
        }
    }

    private Address rom(long v) {
        return romSpace.getAddress(v);
    }

    private void write(String path, String text) throws IOException {
        try (Writer w = new OutputStreamWriter(new FileOutputStream(path), "UTF-8")) {
            w.write(text);
        }
    }
}
