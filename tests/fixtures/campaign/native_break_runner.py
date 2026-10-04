"""Configured adapter to FF9's existing independent CPU, not a new walker."""
import importlib.util,json,struct,sys
def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec);sys.modules[name]=value;spec.loader.exec_module(value);return value
for line in sys.stdin:
    request=json.loads(line)
    if request["op"]=="stop":break
    if request["op"]=="init":
        module("asmgen",request["artifacts"]["cpu-helper"]);cpu=module("independent_cpu",request["artifacts"]["cpu-model"])
        print(json.dumps(dict(ready=True,version="existing-ff9-cpu-break-1",architecture="ps1-mipsel",runnerKind="independent-native-cpu")),flush=True);continue
    if request["arguments"]!=["0x0"]:raise ValueError("fixture supports only the reviewed zero-divisor boundary")
    entry=0x80010000;cursor=0x80001000;words=[0x10800004,0,0xaca80000,0x03e00008,0,0x0000000d]
    mem=bytearray(2*1024*1024);mem[0x10000:0x10018]=b"".join(struct.pack("<I",w) for w in words);mem[0x1000:0x1004]=struct.pack("<I",5)
    addresses=set(range(entry,entry+len(words)*4,4))
    class Observed(cpu.Mips):
        instructions=0;last_pc=entry
        def rd(self,address,n,signed=False):
            if address in addresses and n==4:self.instructions+=1;self.last_pc=address
            return super().rd(address,n,signed)
        def hook(self,*args):raise ValueError("unexpected external/native hook")
    machine=Observed(mem,addresses,{entry},set());regs=[0]*32;regs[4]=0;regs[5]=cursor;regs[8]=6;regs[31]=cpu.RET
    try:machine.run(entry,regs,maxsteps=32);raise ValueError("expected actual BREAK was not reached")
    except cpu.Fault as error:
        if str(error)!="break":raise
    execution=dict(status="exception",instructions=machine.instructions,returnWord=None,exception=dict(kind="checked-divide-trap",operation="checked-divide",guestPc=hex(machine.last_pc),checkpoint="exception-point"),ram=[dict(id="cursor",start=hex(cursor),data="0x"+mem[0x1000:0x1004].hex())],registers={},events=[dict(kind="exception",pc=hex(machine.last_pc),provider="checked-divide")],checkpoints={"exception-point":dict(cursor=hex(int.from_bytes(mem[0x1000:0x1004],"little")))},coverage=["checked-divide:zero-divisor"])
    print(json.dumps(dict(id=request["id"],execution=execution)),flush=True)
