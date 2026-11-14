// YAP_DEBUG_SAMPLE=<file prefix> (diagnostics, docs/PERFORMANCE.md pass 6):
// a CPU sampling profiler of the GUI thread, without elevation (the
// alternative to WPR/xperf, which needs it).
//
// Each time "<prefix>.now" appears, a thread of ours samples the GUI thread
// every millisecond for YAP_DEBUG_SAMPLE_SECONDS (default 20): it suspends
// it, unwinds its stack with the modules' unwind tables
// (RtlVirtualUnwind), and resumes it. The stacks go to "<prefix>.<tag>.txt"
// (frames as exe+offset, or a DLL's nearest export) for
// tools/p6/samplesym.py, which names them from the link map and leaves out
// the stacks parked in a wait. Off unless the variable is set.
#pragma once

namespace sampler {

// Call on the GUI thread: the thread to sample. A no-op unless
// YAP_DEBUG_SAMPLE is set.
void install();

} // namespace sampler
