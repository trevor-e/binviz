__declspec(dllexport) unsigned gte_d[32],gte_c[32],gte_unimplemented;
__declspec(dllexport) void gte_mtc2(int n,unsigned v){gte_d[n]=v;} __declspec(dllexport) void gte_ctc2(int n,unsigned v){gte_c[n]=v;}
__declspec(dllexport) unsigned gte_mfc2(int n){return gte_d[n];} __declspec(dllexport) unsigned gte_cfc2(int n){return gte_c[n];}
__declspec(dllexport) void gte_cop2(unsigned w){if(w!=1){gte_unimplemented=1;return;}gte_d[0]+=gte_c[0];}
