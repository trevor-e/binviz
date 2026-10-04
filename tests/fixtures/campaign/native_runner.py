"""Real Windows x64 native execution; instrumented device observations.
Instruction counts are unavailable (zero), and no PS1 instruction proof is claimed.
"""
import ctypes
import json
import sys

def word(v):return hex(v & 0xffffffff)
for line in sys.stdin:
    request=json.loads(line)
    if request["op"]=="stop":break
    if request["op"]=="init":
        dll=ctypes.CDLL(request["artifacts"]["native-module"])
        for name in ["compute","get_ram","get_trace","get_trace_count"]:
            fn=getattr(dll,name);fn.restype=ctypes.c_uint;fn.argtypes=[] if name=="get_trace_count" else [ctypes.c_uint]
        reply=dict(ready=True,version="synthetic-native-1",architecture="windows-x64",coverageKind="instrumented-source",providerProfile="real native compute; controlled synthetic device")
    else:
        value=int(request["arguments"][0],16);result=dll.compute(value)
        events=[dict(kind="call",pc="0x0",provider="compute",profile="real",arguments=[word(value)],address=None,value=None,delaySlot=False)]
        for i in range(dll.get_trace_count()):events.append(dict(kind="device",pc="0x4",provider=None,profile="controlled",arguments=[],address="0x1",value=word(dll.get_trace(i)),delaySlot=False))
        events.append(dict(kind="return",pc="0x8",provider=None,profile=None,arguments=[],address=None,value=word(result),delaySlot=False))
        reply=dict(id=request["id"],execution=dict(status="executed",reason=None,instructions=0,returnWord=word(result),ram=[dict(id="state",start="0x0",data="0x"+bytes([dll.get_ram(0),dll.get_ram(1)]).hex())],registers={},events=events,checkpoints={"after-device":dict(count=dll.get_trace_count())},coverage=["compute:short-extension"],localObjects=[]))
    print(json.dumps(reply),flush=True)
