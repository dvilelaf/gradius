// Query one or more addresses: instruction, function, data, xrefs, pointer occurrences
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.scalar.Scalar;

public class QueryAddr extends GhidraScript {
    @Override
    public void run() throws Exception {
        Program p = currentProgram;
        Listing lst = p.getListing();
        ReferenceManager rm = p.getReferenceManager();
        Memory mem = p.getMemory();
        for (String arg : getScriptArgs()) {
            long v = Long.parseLong(arg.replace("0x", "").replace("$", ""), 16);
            Address a = p.getAddressFactory().getDefaultAddressSpace().getAddress(v);
            Address ra = null;
            for (MemoryBlock b : mem.getBlocks())
                if (b.isInitialized() && b.isExecute()) {
                    ra = b.getStart().getAddressSpace().getAddress(v);
                    break;
                }
            Address q = (ra != null) ? ra : a;
            println("== " + arg);
            Instruction ins = lst.getInstructionContaining(q);
            println("  instruction: " + (ins == null ? "-" : ins.getAddress() + "  " + ins));
            Function f = lst.getFunctionContaining(q);
            println("  function: " + (f == null ? "-" : f.getName() + " @ " + f.getEntryPoint()
                + " size 0x" + Long.toHexString(f.getBody().getNumAddresses())));
            Data d = lst.getDataAt(q);
            if (d != null) println("  data: " + d.getDataType().getName());
            for (Reference r : rm.getReferencesTo(q)) {
                Function rf = lst.getFunctionContaining(r.getFromAddress());
                println("  xref from " + r.getFromAddress() + " (" + r.getReferenceType() + ")"
                    + (rf == null ? "" : " in " + rf.getName()));
            }
            // pointer occurrences (16-bit LE)
            if (mem.contains(q)) {
                for (MemoryBlock b : mem.getBlocks()) {
                    if (!b.isInitialized()) continue;
                    long n = b.getSize();
                    for (long i = 0; i + 1 < n; i++) {
                        Address x = b.getStart().add(i);
                        if (lst.getInstructionContaining(x) != null) continue;
                        if ((mem.getByte(x) & 0xFF) == (v & 0xFF)
                                && (mem.getByte(x.add(1)) & 0xFF) == ((v >> 8) & 0xFF)) {
                            println("  pointer word at " + x);
                        }
                    }
                }
            }
        }
    }
}
