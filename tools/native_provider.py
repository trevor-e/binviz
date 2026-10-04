"""Pinned COP2 backend callbacks for an existing executor's injection interface.

The executor owns instruction decoding, branch/load delays and RAM. This module
never replaces or rewrites its run method. A software backend is not a hardware
oracle. Export these callbacks through set_cop2_provider(callbacks) in a runner.
"""
import ctypes
import hashlib
import json
from pathlib import Path

OPERATIONS=['read-data','write-data','read-control','write-control','command','load','store']

class Cop2Provider:
    def __init__(self,selection,artifacts):
        self.selection=selection
        if selection.get('kind')!='cop2' or selection.get('authority') not in ('software-model','hardware-oracle') or selection.get('operations')!=OPERATIONS:raise ValueError('complete reviewed COP2 selection required')
        self.profile=json.loads(Path(artifacts[selection['profileArtifact']]).read_bytes())
        if self.profile.get('selection')!=selection:raise ValueError('backend profile differs from selected source/compiler/authority')
        for key in ('artifact','sourceArtifact','compilerArtifact'):
            raw=Path(artifacts[selection[key]]).read_bytes()
            if hashlib.sha256(raw).hexdigest()!=self.profile.get('sha256',{}).get(selection[key]):raise ValueError('native provider pinned input drift: '+key)
        self.allowed=set(self.profile['commands']);self.budget=int(self.profile['operationBudget'])
        if not self.allowed or self.budget<=0:raise ValueError('reviewed commands and positive provider operation budget required')
        self.lib=ctypes.CDLL(str(Path(artifacts[selection['artifact']]).resolve()))
        for name in ('gte_mtc2','gte_ctc2'):
            f=getattr(self.lib,name);f.argtypes=[ctypes.c_int,ctypes.c_uint32];f.restype=None
        for name in ('gte_mfc2','gte_cfc2'):
            f=getattr(self.lib,name);f.argtypes=[ctypes.c_int];f.restype=ctypes.c_uint32
        self.lib.gte_cop2.argtypes=[ctypes.c_uint32];self.lib.gte_cop2.restype=None
        self.data=(ctypes.c_uint32*32).in_dll(self.lib,'gte_d');self.control=(ctypes.c_uint32*32).in_dll(self.lib,'gte_c')
        self.unknown=ctypes.c_uint32.in_dll(self.lib,'gte_unimplemented');self.events=[];self.count=0
    def reset(self,data,control):
        if len(data)!=32 or len(control)!=32:raise ValueError('complete initial COP2 data/control banks required')
        # Raw initial state preserves aliases/sign bits; callbacks apply backend semantics.
        for i in range(32):self.data[i]=data[i];self.control[i]=control[i]
        self.unknown.value=0;self.events=[];self.count=0
    def record(self,operation,pc,args,value=None,address=None,delay_slot=False):
        self.count+=1
        if self.count>self.budget:raise ValueError('native provider operation budget exceeded')
        e=dict(kind='device',pc=hex(pc),provider=self.selection['artifact'],profile=self.selection['authority'],arguments=[hex(OPERATIONS.index(operation)),*[hex(a & 0xffffffff) for a in args]],delaySlot=delay_slot)
        if value is not None:e['value']=hex(value & 0xffffffff)
        if address is not None:e['address']=hex(address & 0xffffffff)
        self.events.append(e)
    @staticmethod
    def register(n):
        if not isinstance(n,int) or not 0<=n<32:raise ValueError('COP2 register outside bank')
        return n
    def read_data(self,n,pc,delay_slot=False):
        value=self.lib.gte_mfc2(self.register(n));self.record('read-data',pc,[n],value,delay_slot=delay_slot);return value
    def read_control(self,n,pc,delay_slot=False):
        value=self.lib.gte_cfc2(self.register(n));self.record('read-control',pc,[n],value,delay_slot=delay_slot);return value
    def write_data(self,n,value,pc,delay_slot=False):
        self.record('write-data',pc,[self.register(n),value],delay_slot=delay_slot);self.lib.gte_mtc2(n,value)
    def write_control(self,n,value,pc,delay_slot=False):
        self.record('write-control',pc,[self.register(n),value],delay_slot=delay_slot);self.lib.gte_ctc2(n,value)
    def command(self,word,pc,delay_slot=False):
        if word not in self.allowed:raise ValueError('unknown or unreviewed COP2 command: '+hex(word))
        self.record('command',pc,[word],delay_slot=delay_slot);self.lib.gte_cop2(word)
        if self.unknown.value:raise ValueError('native backend reported unimplemented COP2 command')
    def load(self,n,address,read_word,pc,delay_slot=False):
        value=read_word(address);self.record('load',pc,[self.register(n)],value,address,delay_slot);self.lib.gte_mtc2(n,value)
    def store(self,n,address,write_word,pc,delay_slot=False):
        value=self.lib.gte_mfc2(self.register(n));self.record('store',pc,[n],value,address,delay_slot);write_word(address,value)
    def final_state(self):
        if self.unknown.value:raise ValueError('native provider has an unresolved command')
        return dict(authority=self.selection['authority'],data=list(self.data),control=list(self.control),operations=self.count,profile=self.selection['profileArtifact'])
    def install(self,executor):
        setter=getattr(executor,'set_cop2_provider',None)
        if not callable(setter):raise ValueError('existing executor has no complete COP2 injection interface; extend its callbacks explicitly')
        callbacks={op:getattr(self,op.replace('-','_')) for op in OPERATIONS}
        installed=setter(callbacks)
        if installed!=OPERATIONS:raise ValueError('executor did not acknowledge all seven COP2 callbacks')
        return callbacks
