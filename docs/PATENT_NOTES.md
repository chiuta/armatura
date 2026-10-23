# Patent notes

**This is not legal advice and not a freedom-to-operate opinion.** It records what has NOT been checked, so nobody mistakes the absence of a screening for a clearance. ARMATURA's licences (MPL-2.0 for the libraries, AGPL-3.0-or-later for the CLI) contain contributor patent grants for the author's own code, but they say nothing about third parties' patents. No patent search by a professional has been done.

## Screening still undone

| Area | Why it may matter | Status |
|---|---|---|
| Face-driven puppeteering (`faceparam`, `facedet`, `liveinput`: face landmarks or ratios driving rig parameters) | Performance-driven animation of 2D characters is an active field of commercial filings. | Not screened. The code uses published textbook measures (eye aspect ratio, One-Euro filter, HOG + logistic regression); that does not exclude claims on the combination. |
| State machines and data binding (`statemachine`, `runtime_machine`, `bind`) | Interactive-animation runtimes with state machines, blend trees, inputs and view-model binding are a commercial feature area. | Not screened. Own design of the data model, not a patent clearance. |
| FABRIK, mean-value coordinates, bounded biharmonic weights (`rig`, `cage`, `weights`) | Published algorithms; some authors and employers filed patents around deformation and skinning techniques. | Not screened. FABRIK (Aristidou and Lasenby, 2011) and MVC (Floater, 2003) are used from the papers; the weights module uses plain harmonic diffusion, not the published bounded biharmonic solver. |
| Stencil-then-cover path filling (`armatura-gpu`) | The technique originates in GPU path-rendering research and vendor extensions. | Not screened. Used here only through the standard wgpu API with a plain stencil buffer. |
| Video and audio codecs through ffmpeg (`export` .mp4 / .webm, `soundtrack`) | H.264 and AAC are covered by licensing pools (MPEG LA / Via LA and others). VP9 and AV1 are marketed as royalty-free, but patent-pool and individual patent claims have been announced for both. | Not assessed. Do not treat VP9/AV1 as cleared. |

## ffmpeg
ffmpeg is an **external subprocess** supplied by the user. ARMATURA does not link it, does not ship it and must not bundle it in any release: bundling would make the distributor responsible for ffmpeg's own licence (LGPL/GPL depending on the build) and for the codec licences above. Every codec option is the user's own choice on the user's own installation.

## Before commercial release
Have a patent attorney review the areas above, in the jurisdictions where the software will be distributed, and record the result here and in CLEANROOM.md (rule 5).
