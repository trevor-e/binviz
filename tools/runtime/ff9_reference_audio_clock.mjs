// Explicit reference audio clock for the owned functional counter scheduler.
// The existing device step is a hardware quantum; faults are terminal after it.
export function createFf9ReferenceAudioClock({device,createSampleClock,getCdInput,observe=()=>{}}={}){
 const demand=(c,s)=>{if(!c)throw Error('FF9 reference audio clock: '+s);};
 demand(device?.step&&device?.snapshot&&device?.mixerSnapshot&&typeof createSampleClock==='function'&&typeof getCdInput==='function','owned device, sample converter and explicit CD authority');
 const clock=createSampleClock(device);let attempts=0,accepted=0,failed=false,lastError=null;
 const snapshot=()=>({attempts,accepted,failed,lastError,clock:clock.snapshot(),device:device.snapshot(),mixer:device.mixerSnapshot(),referenceOnly:true,hardwareTimingVerified:false});
 const advance=(...args)=>{
  demand(args.length===1&&Number.isSafeInteger(args[0])&&args[0]>=0&&args[0]<=768*4096,'bounded exact system-clock ABI');
  demand(!failed,'terminal audio clock');const systemClocks=args[0];if(systemClocks===0)return;
  attempts++;observe('before',snapshot());
  try{
   const samples=Math.floor((clock.snapshot().remainderClocks+systemClocks)/768);
   const cdPcm=samples?getCdInput(samples):undefined;
   device.step();const value=clock.advance(systemClocks,{cdPcm});accepted++;
   observe('after',snapshot(),value.output);return value;
  }catch(error){failed=true;lastError=error.message;observe('failed',snapshot());throw error;}
 };
 return Object.freeze({advance,snapshot});
}
