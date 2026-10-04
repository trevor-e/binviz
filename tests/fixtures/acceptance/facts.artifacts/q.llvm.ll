; ModuleID = '/workspace/tests/fixtures/acceptance/facts.artifacts/q.prepared.i'
source_filename = "/workspace/tests/fixtures/acceptance/facts.artifacts/q.prepared.i"
target datalayout = "e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-n32:64-S128-ni:1:10:20"
target triple = "wasm32-unknown-unknown"

; Function Attrs: mustprogress nofree norecurse nosync nounwind readnone willreturn
define hidden i32 @provider(i32 noundef %0) local_unnamed_addr #0 {
  %2 = add nsw i32 %0, 1
  ret i32 %2
}

attributes #0 = { mustprogress nofree norecurse nosync nounwind readnone willreturn "frame-pointer"="none" "min-legal-vector-width"="0" "no-trapping-math"="true" "stack-protector-buffer-size"="8" "target-cpu"="generic" }

!llvm.module.flags = !{!0}
!llvm.ident = !{!1}

!0 = !{i32 1, !"wchar_size", i32 4}
!1 = !{!"Debian clang version 14.0.6"}
