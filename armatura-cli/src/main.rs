// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// Output that cannot be delivered (a closed pipe: `armatura ... | head`; a closed stderr) must never panic or
// change the exit status: these macros replace the std ones for the whole crate and drop what cannot be written.
macro_rules! println {
    () => {{ use std::io::Write as _; let _ = std::io::stdout().lock().write_all(b"\n"); }};
    ($($t:tt)*) => {{ use std::io::Write as _; let _ = writeln!(std::io::stdout().lock(), $($t)*); }};
}
#[allow(unused_macros)]
macro_rules! print {
    ($($t:tt)*) => {{ use std::io::Write as _; let _ = write!(std::io::stdout().lock(), $($t)*); }};
}
macro_rules! eprintln {
    () => {{ use std::io::Write as _; let _ = std::io::stderr().lock().write_all(b"\n"); }};
    ($($t:tt)*) => {{ use std::io::Write as _; let _ = writeln!(std::io::stderr().lock(), $($t)*); }};
}
#[allow(unused_macros)]
macro_rules! eprint {
    ($($t:tt)*) => {{ use std::io::Write as _; let _ = write!(std::io::stderr().lock(), $($t)*); }};
}
mod http;
mod collab_srv;
mod soundio;
mod vcs_cmd;
mod fuzz_cmd;
mod ci_cmd;
mod face_cmd;
mod fit_cmd;
mod lod_cmd;
mod studio;
mod uiconfig;

use armatura_core::anim_tools::{mirror, walk_cycle, WalkRig, WalkStyle};
use armatura_core::audio::read_wav_file;
use armatura_core::edit::Editor;
use armatura_core::export;
use armatura_core::{autotrace, posesuggest, tinymlp, speech, links, warp, cage, clipstack, cloth, soundtrack, looper, bonevis, parentswitch, skins, audiomap, lottie_import, hdr, seqimport, chanmeta, depth, eyeclip, physics, magnet, footlock, motionlib, camshots, pathfollow, layout, autorig, drivers, comps, explain, rigedit, snapshots, springfit, vocab, agent, overlays, postfx, importers, inspect, keyops, lint, procgen, open_formats, perf, posematch, safety, statemachine, trace, meshtrace, weights};
use armatura_core::project::{Prop, Source};
use armatura_core::math::Vec2;
use armatura_core::raster::{render_png, RasterOptions};
use armatura_core::svg::{render_frame, RenderOptions};
use armatura_core::{demo, Engine, Project};
use std::collections::HashMap;
use std::{env, fs, path::Path, process::{Command, ExitCode}};

const USAGE: &str = "\
armatura — 2D rigging & animation tools (independently implemented; see CLEANROOM.md)

GLOBAL OPTIONS (accepted anywhere on the line, also after a sub-command; `--key=value` works for every value option):
  --allow-external-assets     let a project name files by absolute path or outside its own directory (default: refused)
  --follow-symlinks           write to the target of a symbolic link given as an output file (default: refused)
  --max-input-bytes N         largest project file that is read (default 268435456); devices and pipes are never read
  <command> --help            the usage of one command;  <command> --version  the version

USAGE:
  armatura --version   print the version, licence (AGPL-3.0-or-later), source offer and support links
  armatura demo     <out.json>                         write the built-in rig demo
  armatura demo-fx  <out.json>                         write the liquid / boolean / particles demo
  armatura check    <project.json>                     validate a project
  armatura render   <project.json> <frame> <out.svg|out.png> [--bones] [--scale N] [--clamp] [--proxy N]
                    --proxy N: scrub proxy (N = 1..4): .png = bitmap at 1/2^N resolution, .svg = simplified vectors; both drop fine strokes, gradients, blends
  armatura frames   <project.json> <out_dir> [--png] [--bones] [--scale N]
  armatura export   <project.json> <out.gif> [--scale N] [--report r.json]   looping GIF; limits are warned on stderr and listed in --report
  armatura export   <project.json> <out.html|out.htm>   one self-contained HTML player (no options)
  armatura export   <project.json> <out.json|out.lottie> [--report r.json]   .json = Lottie, .lottie = dotLottie; limits are warned on stderr and listed in --report
  armatura export   <project.json> <out.mp4> [--scale N]   H.264, needs ffmpeg on PATH
  armatura lipsync  <project.json> <audio.wav> <switch_layer> <out.json> [--map 0,1,2,3,4]   default map follows the switch's child count; shapes rest,small,open,round,wide: 1 child all, 2 closed+open, 3 closed+open+round/wide, 4 closed+small+open+round/wide, 5+ in that order
  armatura walk     <project.json> <out.json> --thigh-l B --thigh-r B [--shin-l B --shin-r B
                    --arm-l B --arm-r B --hips B --period N --stride DEG --knee DEG --bob PX --swing DEG]
  armatura mirror   <project.json> <out.json>          mirror all keyed channels left<->right
  armatura studio   <project.json> [--port N] [--ui-config F] [--bind ADDR] [--allow-host NAMES] [--public-origin URL] [--source-url URL] [--collab-token T (16+ characters)] [--collab-secret-file F [--collab-revoke NAMES]] [--collab-rate N] [--collab-log LOG [--collab-nosync]]   browser editor with undo/redo (UI layout and shortcuts are kept in F, default ~/.config/armatura/ui.json); LOG persists the shared session and resumes it after a restart
  armatura collab-token <name> [--secret-file F] [--ttl 28800]   mint a per-participant token for a studio started with --collab-secret-file F (HMAC-SHA-256; bound to the name; default life 8 hours, at most 30 days; without --secret-file the secret comes from the environment variable ARMATURA_COLLAB_SECRET)
  armatura collab-sim <project.json> [--clients N] [--ops M] [--seed S] [--undo-pct P] [--replays K] [--require-valid] [--raw] [--json]   N replicas edit concurrently with random delivery order and duplicates, then verify they all hold the same document (exit 1 if not). default edits are valid project edits (numbers, key values, visibility, own layers) so ok can be true; --raw uses arbitrary JSON edits instead (usually not a valid project); JSON ok also needs the merged document to compile; converged_ok is convergence alone; --require-valid makes a non-compiling result exit 1 too
  armatura fuzz     <project.json> [--samples N] [--seed S] [--strategy random|lhs|halton|sweep|corners|pca] [--frame F] [--report out.json] [--heatmap out.svg] [--minimize] [--gate] [--max-fail-rate R] [--props angle,scale,offx,offy] [--only BONES] [--margin DEG] [--db posedb.json] [--ignore KINDS] [--no-deformers]   find poses where the rig breaks (folds, stretch, spikes, self-intersection, IK unreachable, limits, NaN, off-canvas), stress heat-map, minimal reproducer; a report (exit 0) unless --gate (ceiling 0) or --max-fail-rate R (fraction 0..1) is given, then exit 1 above the failure rate
  armatura fuzz-grid <project.json> --x PARAM --y PARAM [--res N] out.svg|out.png|out.json   severity map of two parameters, e.g. bone:arm_up:angle
  armatura vcs      init <project.json> [--author A]   version control with semantic merge for a project file (see: armatura vcs)
  armatura vcs      commit [-m MSG | --message MSG] [--author A] [--repo DIR]
  armatura vcs      log [REV] [--limit N] [--json] [--repo DIR]
  armatura vcs      status [--json] [--repo DIR]
  armatura vcs      diff [A [B]] [--json] [--repo DIR]
  armatura vcs      show [REV] [--json] [--repo DIR]
  armatura vcs      branch [NAME [AT] | -d NAME] [--repo DIR]
  armatura vcs      tag [NAME [AT]] [--repo DIR]
  armatura vcs      checkout REV [--force] [--repo DIR]
  armatura vcs      merge REV [--no-ff] [-m MSG | --message MSG] [--author A] [--abort] [--repo DIR]
  armatura vcs      cherry-pick|revert REV [--author A] [--repo DIR]
  armatura vcs      resolve PATH [--ours] [--theirs] [--value V] [--repo DIR]   exactly one of the three
  armatura vcs      fsck|gc [--repo DIR]
  armatura ui       commands|shortcuts [--ui-config F] [--json]    studio command registry / effective shortcuts and conflicts
  armatura ui       layout presets|show <name>|validate <file.json>   dock layout presets (animate, rig, compact, live)
  armatura ui       search <query...> [--ui-config F]              fuzzy command search as the palette ranks it

OPEN FORMATS (open specifications; no H.264/H.265 unless you ask for .mp4; codec patent status is not assessed, see docs/PATENT_NOTES.md)
  armatura export   <project.json> <out.apng> [--scale N]   animated PNG
  armatura export   <project.json> <out.webm> [--scale N] [--codec vp9|av1]   VP9 with alpha (--codec av1 for AV1) via ffmpeg
  armatura export   <project.json> <out.png> [--scale N] [--columns N]   sprite sheet + <out>.atlas.json (layout; never <out>.json)
  armatura export   <project.json> <out.glb> [--report r.json]   glTF 2.0 (bones, animation, skinned fills/strokes, gradients, masks, images, morph targets)
  armatura export-oca <project.json> <out_dir> [--layers]   Open Cel Animation folder
  armatura import-ora <file.ora> <out.json> [--fps N] [--frames N]   OpenRaster -> project

SAFETY
  armatura safety   <project.json> [--scale N]         flash / red-flash / camera-motion report (WCAG 2.3.1 approximation)
  armatura reduced-motion <project.json> <out.json> [--factor 0.25]   calmer copy (camera + wobble scaled)

INSPECT / QUALITY
  armatura stats    <project.json>                     counts of bones, layers, channels, keys (always printed as JSON)
  armatura diff     <a.json> <b.json>                  semantic differences (+ added, - removed, ~ changed)
  armatura golden   write <project.json> <golden.json> [--scale N]   record hash + thumbnail of every frame; N in (0, 1] (larger values are refused)
  armatura golden   check <project.json> <golden.json> [--tol T]   regression frames: exact by default (any pixel change fails, exit 1); --tol T accepts a mean thumbnail difference up to T (0..1); the scale stored in the golden is used (a scale option is refused here)
  armatura export-fit <project.json> <out> --max-size 500KB [--format lottie|html|gif|apng|webm|project] [--min-ssim X] [--max-seconds 60] [--measure raw|gzip] [--frames N] [--search ladder|descent|grid] [--report out.json] [--pareto out.csv] [--audit] [--no-baseline]   best-quality export that fits a byte budget (--max-seconds is a real wall-clock budget; SSIM against the original render; exit 1 and the smallest export if the budget cannot be met)
  armatura size-report <project.json> [--tol T] [--json]   which layers, images and channels contribute bytes (project file, baked Lottie, HTML page)
  armatura lod      <project.json> --tolerance-px 0.5 [--scale S | --size WxH] [--levels 4] [--ratio 2] [--out-dir D] [--report r.json] [--verify] [--frames N]   level of detail L1..Ln with a declared error bound in output pixels (path simplification, key reduction, mesh collapse, culling); --verify measures the real deviation (exit 1 if above the bound)
  armatura lod-pick <project.json> --screen-px WxH [--max-px 0.5] [--tolerance-px 0.25] [--levels 4] [--out chosen.json]   coarsest level whose declared bound stays within the pixel budget on that screen
  armatura ci       init <project.json> [--out armatura-ci.json] [--no-goldens] | run [manifest] [--update] [--bless] [--prune] [--only a,b] [--junit f.xml] [--html f.html] [--json f.json] [--artifacts dir] [--quiet] | bless [manifest] | diff a.png b.png [--metric ssim|de2000|de76|mse] [--tol T] [--ignore x,y,w,h;...] [--aa N] [--out heat.png] [--side s.png] [--overlay o.png] [--flicker f.gif]   animation CI: golden frames with perceptual diff (SSIM, CIEDE2000), structural checks, budgets; exit 0 pass, 1 fail, 2 error
  armatura lint     <project.json>                     static rig / animation checks (exit 1 on errors)
  armatura contracts <project.json> <contracts.json>   motion contracts (unit tests for animation)
  armatura loop-solve <project.json> <out.json> [--window 24] [--inclusive]   make keyed channels loop seamlessly (period N frames, no duplicated seam frame; --inclusive = last frame equals first, legacy)
  armatura coach    <project.json>                     timing / principles suggestions
  armatura timecode <frame|HH:MM:SS;FF> [--fps 24] [--drop] [--nodrop]   SMPTE timecode (29.97/59.94 = drop-frame with ';' by default; --nodrop counts 30; a timecode argument prints its frame)
  armatura mcp      <project.json>                     agent server (JSON-RPC over stdin/stdout; tools to inspect, edit, lint, render, save)

RIG / PROJECT ORGANISATION
  armatura tag      <project.json>                     propose standard roles (arm.upper.l, face.mouth…) for bones and layers
  armatura autorig <project.json> <out.json> [seed]   IK (mix 0), breathing, blinking from bone/layer names
  armatura slider <project.json> <out.json> <name> <poses>   poses: at:target.prop=val,target.prop=val;at:...
  armatura view-swap <project.json> <out.json> <switch_layer> <bone.prop> <from:child,from:child,...>
  armatura align <project.json> <out.json> <mode> <layer,layer,...|layer> [stage]   mode: left right top bottom center-x center-y
  armatura distribute <project.json> <out.json> <x|y> <gaps|centers> <layer,layer,...>
  armatura trim <project.json> <out.json> <layer> <path_index> <start> <end> [offset]   bakes the trimmed part
  armatura shot <project.json> <out.json> <name> <x> <y> <zoom> [rotation]   save a camera shot
  armatura camera-seq <project.json> <out.json> <frame:shot[:cut],...>   keys the camera through saved shots
  armatura sound list <project.json> | add <project.json> <out.json> <name> <file.wav|ogg|mp3|...> [start_frame] [gain] [trim/fade options] | edit <project.json> <out.json> <name> [--start F] [--gain G] [trim/fade options] | remove <project.json> <out.json> <name>
                    trim/fade options (seconds, or frames with an f suffix such as 36f): --trim-in T --trim-out T --fade-in T --fade-out T (trim points are inside the audio file; edit: 0 clears)
  armatura sound mix <project.json> <out.wav|out.ogg|out.mp3>   only the mixed audio (start, gain, trim, fades; project length), through ffmpeg
                    audio is mixed into .mp4 (AAC) and .webm (Opus) and embedded in .html (needs ffmpeg); .gif and .apng cannot carry audio and stay silent
  armatura make-loop <project.json> <out.json> [targets|all] [props] [--inclusive]   (removes travel = in-place; period N frames so playing 0..N-1 repeatedly has no duplicate seam frame; --inclusive = frame N-1 equals frame 0, legacy)
  armatura follow <project.json> <out.json> <layer|bone:NAME> <layer_with_path> <start_frame> <end_frame> [no-orient]
  armatura take-record <project.json> <library.json> <name> <from> <to> [target_prefix,...]   library is created if missing
  armatura take-apply <project.json> <out.json> <library.json> <name> <at> [from=to,...] [speed] [--blend FRAMES]   blend cross-fades with existing animation at both ends
  armatura take-list <library.json>
  armatura clip add <project.json> <out.json> <library.json> <take> <clip> [--at F --speed S --loops N --mode replace|additive|multiply --weight W --include a_*,b --exclude c --track-weight arm_*=1,leg_*=0.2 --rename a=b --fade-in F --fade-out F]   layer a take on the timeline
  armatura clip list <project.json>                    the clip stack (later clips layer over earlier ones)
  armatura clip remove <project.json> <out.json> <clip>
  armatura clip set-weight <project.json> <out.json> <clip> [W] [--keys frame:w,frame:w] [--track PATTERN[:prop]]   constant, keyed, or per-track weight
  armatura clip bake <project.json> <out.json>         regenerate the stack-owned channels (hand-made and locked channels are never touched)
  armatura machine add <project.json> <out.json> <machine.json>   add a runtime state machine (inputs, states, transitions, blend trees, layers); validated
  armatura machine list <project.json>  |  machine remove <project.json> <out.json> <name>  |  machine validate <project.json>
  armatura machine-sim <project.json> <script.json> <out_dir|out.json> [--fps N --svg-frames N]   replay timed input events: state log (sim.json) + SVG frames
  armatura container add <project.json> <out.json> <container.json>   add a layout container (row/column, gap, padding, justify, align, wrap, grow/shrink, nested, canvas-relative rect); validated
  armatura container list <project.json>  |  container remove <project.json> <out.json> <id>  |  container solve <project.json> [--size WxH --data out.json]   print solved rectangles and layer offsets
  armatura scroll add <project.json> <out.json> <scroll.json>  |  scroll list <project.json>  |  scroll remove <project.json> <out.json> <id>   scroll regions (viewport, content container, clamp, elastic, snap, clip layer)
  armatura scroll eval <project.json> <id> [--offset X,Y --size WxH --velocity VX,VY --time S]   clamped/elastic offset; with --velocity simulate the release (momentum, snap, spring)
  armatura layout-preview <project.json> <out.svg> --size WxH [--frame N --scroll id=X,Y]   render with layout re-solved for another canvas size, container outlines drawn
  armatura bind add <project.json> <out.json> <name> <value.path> <target:prop|@machine:input> [--expr E --remap a,b,c,d --no-clamp --smooth S --mode replace|add|multiply --weight W]   data binding
  armatura bind list <project.json>  |  bind remove <project.json> <out.json> <name>  |  bind-eval <project.json> <values.json> [--data out.json]   evaluate bindings for given values
  armatura foot-lock <project.json> <out.json> <ctrl_bone> <ground_y> [tolerance] [min_frames]   pins a root controller during ground contact
  armatura stretch <project.json> <out.json> <follower_bone> <leader_bone> [min=1] [max=3]   follower aims at the leader's head and stretches to reach it (baked, uniform scale)
  armatura magnet <project.json> <out.json> <ctrl_bone> <bone:NAME|x,y> <radius> <strength 0..1>   bakes a proximity pull
  armatura cage <project.json> <out.json> <layer> [rect=x,y,w,h|auto] [hull] [verts=N] [pad=P] [name=cage]   adds a mean-value-coordinate cage deformer around a layer (auto = padded bounds, verts = segments per side; hull = convex hull); content inside follows the cage, outside is unchanged
  armatura cage-pose <project.json> <out.json> <layer> <frame> [name=cage] [move=i:dx,dy;...] [all=dx,dy] [ease=linear]   keys cage vertex offsets at a frame (cage_dx/cage_dy channels; locked channels are refused)
  armatura warp-pivot <project.json> <out.json> <layer> <frame> [deformer=NAME] [verts=1,4-7] [region=x,y,w,h] [pivot=x,y|bone:NAME|centroid|center] [rotate=DEG] [scale=S|sx,sy] [move=dx,dy] [radius=R] [falloff=smooth] [ease=linear]   rotates/scales/moves a selection of control points (cage, lattice, cloth, pin, spine; a grid image mesh gets a per-vertex warp grid) about a pivot and keys the result at the frame; radius adds a soft falloff (linear, smooth, gauss, sharp, constant)
  armatura soft-scale <project.json> <out.json> <layer> <frame> [deformer=NAME] [verts=..|region=..] scale=S|sx,sy [radius=R] [falloff=smooth] [pivot=..]   proportional (soft-selection) scaling of the selection; without verts/region the whole mesh is resized about its bounding-box centre: scale=F | width=W | height=H | size=WxH, aspect=keep|free
  armatura layer-pivot <project.json> <out.json> <layer> pivot=x,y|bone:NAME   moves the layer pivot (anchor) without changing the picture, so rotation/scale channels turn about it
  armatura mesh-lock add <project.json> <out.json> <name> host=LAYER[:path] follower=LAYER[:path] [verts=1,4-7] [strength=1]   follower vertices follow the host mesh surface (barycentric displacement transfer) every frame; cycles are refused
  armatura mesh-lock list|remove <project.json> [<out.json> <name>]   lists / removes mesh locks
  armatura glue add <project.json> <out.json> <name> a=LAYER[:path] b=LAYER[:path] pairs=i:j;i:j|auto=RADIUS [mode=rigid|snap|offset] [strength=1]   glues b vertices to a vertices (b follows a); rigid keeps the rest offset in the frame of a, snap makes them coincide, offset keeps a constant offset
  armatura glue list|remove <project.json> [<out.json> <name>]   lists / removes glues
  armatura cloth <project.json> <out.json> <layer> [pin=top,left@bone,row:N,col:N,v:N,corners,none] [gravity=900] [wind=x,y] [gust=0.5] [gust-hz=0.5] [gust-len=px] [stiffness=1] [shear=0.05] [bend=0.2] [max-stretch=1.1] [damping=0.99] [cols=N rows=N] [rect=x,y,w,h] [name=cloth] [substeps=4] [iterations=10] [preroll=frames] [circle=x,y,r[@bone];...] [capsule=x1,y1,x2,y2,r[@bone];...] [from=F] [to=F]   bakes a deterministic cloth surface onto an image layer mesh (or a vector layer with rect=) as per-vertex deform channels
  armatura physics <project.json> <out.json> <bone,bone,...> [gravity=900] [wind=x,y] [gust=0.5] [damping=0.985] [circle=x,y,r[@bone];...] [capsule=x1,y1,x2,y2,r[@bone];...] [follow=bone] [--allow-empty] [drive=bone:gain;...] [vortex=bone|x,y:swirl[:radial[:radius]];...] [preset=hair|tail|rope|antenna|cape] [merge=keep|replace] [lateral=0..1]   bakes a chain simulation; options after preset= override it. Several chains: bone,bone/bone,bone with lateral= ties neighbouring strands
  armatura eye-clip <project.json> <out.json>   masks iris/pupil layers by the eye shape on the same side
  armatura parallax <project.json> <out.json> <far_layer,...,near_layer> <far_value> <near_value>   multiplane depth for top-level layers
  armatura chan-meta <project.json> <out.json> <target> <prop|*> [color=#rrggbb] [shy=on|off] [solo=on|off] [lock=on|off]
  armatura reference <project.json> <out.json> <layer> <on|off>   tracing layer: shown in the studio, never exported
  armatura import-seq <project.json> <out.json> <layer_name> <folder> [hold=1] [loop=on|off] [start=0] [x=0] [y=0]   PNG sequence -> frame-by-frame layer
  armatura import-anim <project.json> <out.json> <layer_name> <file.gif|file.apng|file.png> [loop=on|off] [start=0] [x=0] [y=0]   animated GIF/APNG -> frame-by-frame layer with the file's delays
  armatura dof <project.json> <frame> <out.png> [--focus 1.0 --strength 12 --max-radius 24 --scale N --clamp]   depth of field: each top-level layer is blurred by |parallax - focus|  (focus/strength follow camera_focus/camera_blur channels unless --focus/--strength is given)
  armatura dof-export <project.json> <out.gif|out.apng|out.mp4|out.webm|out-folder> [--from F --to F --focus N --strength N --max-radius N --scale N --codec vp9|av1]   animated depth of field over a frame range (.mp4/.webm through ffmpeg, without audio; --codec applies to .webm); focus/strength follow the camera_focus/camera_blur keys unless pinned; a folder gets a PNG sequence
  armatura render-gpu <project.json> <out.png> [--frame N] [--adapter software|auto] [--scale S] [--bench N]   GPU render (wgpu); only in builds with --features gpu; unsupported scenes (images, bones) are reported, never drawn wrongly
  armatura render-hdr <project.json> <frame> <out.exr|out.png> [linear] [--precision half|float] [--float32] [--8bit] [--scale N] [--clamp]   f32 render: EXR (HALF or FLOAT, linear, premultiplied) or 16-bit PNG (no dither); --8bit uses the legacy 8-bit source
  armatura import-lottie <in.json> <out.json> [fx.json]   subset import; unsupported parts are listed as warnings; blur / drop shadow / tint / fill become per-layer effects inside the project (fx.json is kept for compatibility and is now an empty chain)
  armatura script <project.json> <out.json> <script.rhai>   sandboxed Rhai: fps() frames() bones() layers() set_key(target, prop, frame, value[, ease]) add_bone(name, parent (empty = root), length, x, y, angle) add_rect(name, x, y, w, h, fill) add_circle(name, x, y, r, fill) add_polygon|add_blob(name, [x,y,...], fill) add_line(name, [x,y,...], colour, width, closed) add_gradient_rect(name, x, y, w, h, top, bottom) add_ik(name, [bones], target, bend) add_aim(name, bone, target[, offset, weight]) add_copy_rotation(name, bone, source, weight) add_pin(name, bone, target, weight) add_group(name[, [layers]]) add_switch(name, [layers]) move_layer(path, group_or_empty) set_parent(bone, parent_or_empty) delete_layer(path) delete_bone(name) set_layer(path, prop, value) set_bone(name, prop, value) add_channel_expr(target, prop, expr[, mode]) add_noise(target, prop, amp, freq, seed[, mode]) add_marker(name, frame[, end]) add_event(name, frame[, data]) bone_info(name) layer_info(path) world(bone, frame) print(x)
  armatura audio-react <project.json> <out.json> <audio.wav> <target> <prop> [gain=1] [offset=0] [floor=0.05] [attack=1] [release=4] [add=on]   loudness drives a property
  armatura live-map <project.json> <events.json|song.mid> <mapping.json> <out.json> [--takes lib.json] [--tol 0.01] [--allow-empty]   recorded live input (MIDI, mouse, gamepad, keys) -> keys, takes, events
  armatura beats <project.json> <out.json> <audio.wav> [sensitivity=1.5] [gap=4] [as=markers|events] [name=beat]
  armatura waveform <audio.wav> <out.json> [buckets=800]   min/max peaks for drawing
  armatura skin <project.json> <out.json> add <group> <name> <layer,layer,...>
  armatura skin <project.json> <out.json> rig <group> <name> <constraint,...|-> <ik,...|->   constraints / IK chains active only while that skin is
  armatura skin <project.json> <out.json> bones <group> <name> <bone,...|->   bones that exist only while that skin is active (collapsed to scale 0 otherwise)
  armatura skin <project.json> <out.json> apply <group> <name>
  armatura skin <project.json> <out.json> schedule <group> <frame:name,frame:name,...>   step keys on layer opacity
  armatura parent-switch <project.json> <out.json> <bone> <frame:parent|none,...>   bakes animated re-parenting without a jump (no IK/constraint/spring bones)
  armatura bone-vis <project.json> <out.json> <bone> <from> <to>|clear   show the bone in overlays only for a frame range
  armatura pick <project.json> --frame N --x X --y Y [--tol PX] [--root] [--bones] [--points a,b] [--locked a,b] [--json]   what is under a canvas point (topmost first)
  armatura select-box <project.json> --frame N (--rect x0,y0,x1,y1 | --lasso x,y;x,y;x,y) [--mode bbox|contain|intersect] [--root] [--bones] [--json]   marquee selection
  armatura timeline <project.json> [--json]   tracks per channel grouped by bone/layer, with keys, locks and colours
  armatura keyedit <project.json> <out.json|-> move --sel SEL [--dt N] [--dv N] [--collision replace|merge|refuse] [--no-snap] [--free]   dope-sheet edits as one undo-safe batch (- = dry run, prints the plan)
  armatura keyedit <project.json> <out.json|-> scale --sel SEL --pivot F --factor X | dup --sel SEL --offset N | delete --sel SEL | ease --sel SEL --ease E | value --sel SEL --value V | smooth --sel SEL [--kind monotone|catmull_rom]
  armatura keyedit <project.json> <out.json|-> copy --sel SEL --clip clip.json | paste --clip clip.json --at F [--remap from_target:prop=to_target:prop]
  armatura keyedit <project.json> <out.json|-> add --target T --prop P --at F [--value V] [--ease E] | ripple --at F --by N --mode insert|delete [--scope 0,1] [--adjust-frames]
  armatura keyedit <project.json> <out.json|-> offset --channels 0,1 --dt N | extrapolate --channels 0,1 --extrapolate loop | loop --channels 0,1 [--end F] | handle --sel SEL --side in|out --t F --v V [--mode free|aligned|auto] | lock --target T [--prop P] [--value on|off]
  (SEL = TARGET:PROP[@F0..F1] items joined by ; or #CHANNEL[@F0..F1] or all; E = linear, step, ease_in, ease_out, ease_in_out, smooth, bounce, bezier:x1,y1,x2,y2 or JSON)
  armatura reparent <project.json> <out.json> <bone> <new_parent|none>   keeps the rest pose
  armatura comp     capture|apply <project.json> <out.json> <name>  named layer states: store the current state as name, or restore it
  armatura comp     list <project.json>                print the names (nothing is written)
  armatura event    <project.json> <out.json> <name> <frame> [--data JSON]
  armatura snapshot save <project.json> <dir> [--keep 20]   numbered history of a project in a directory; only the newest --keep revisions stay
  armatura snapshot list <project.json> <dir>   print the revisions
  armatura snapshot restore <project.json> <dir> <out.json> [--number N]   write revision N (default: the newest)
  armatura snapshot recover <project.json> <dir> <out.json>   write the newest revision that can still be read
  armatura explain  <project.json> <bone-or-layer> <frame>      why is it here? ordered contributions
  armatura spring-fit --start V --samples a,b,c,... [--fps 24]    fit spring frequency/damping to a drawn curve
  armatura compare  <a.json> <b.json> <frame> <out.png> [--gain 4 --scale N]   before | after | difference

EFFECTS / OVERLAYS
  armatura fx       <project.json> <frame> <chain.json> <out.png> [--scale N] [--layer-normals] [--clamp]   render + post-effects chain (relight: normal_map, spot lights); --layer-normals shades each layer with its own normal map
  armatura layer-normal <project.json> <out.json> <layer> <normal.png|none> [--strength S] [--fit stretch|tile|exact] [--green-down]   attach (or remove) a normal map on a layer for relight --layer-normals
                    (blur, drop_shadow, glow, outline, posterize, pixelate, levels, fade)
  armatura color-space <project.json> <out.json> srgb|linear   set how the raster / GIF / video / render-hdr compositors blend: srgb (default, as stored) or linear (linear light)
  armatura onion    <project.json> <frame> <out.png> [--before 2 --after 1 --step 1 --scale N]
  armatura onion    <project.json> <out.svg> --frame N [--before B --after A --step S --keyed --outline --all-parts --layers a,b --tint-before C --tint-after C --opacity X --falloff X --ghosts-only]
                    SVG still: the frame with tinted onion-skin ghosts over it (--keyed = nearest key frames instead of fixed steps)
  armatura trail    <project.json> <bone> <out.svg> [--from F --to F]   motion trail of a bone tip
  armatura trail    <project.json> [out.svg] --target layer:PATH|bone:NAME[:head|tip]|point:LAYER:KIND:A[:B] [--from F --to F --smooth N --json]
                    world-to-screen path of a layer pivot, bone or control point with a tick per frame and key markers
  armatura proxy-bench <project.json> [--frames N]   milliseconds per frame, full quality against each proxy level

EDITING / GENERATORS / IMPORT
  armatura keys     shift <project.json> <out.json> --by N [--from F --to F --target NAME --prop PROP]   move keys by N frames
  armatura keys     scale <project.json> <out.json> --factor X [--pivot F --from F --to F --target NAME --prop PROP]   scale key times about a pivot frame
  armatura keys     ease <project.json> <out.json> --ease linear|step|ease_in|ease_out|ease_in_out|smooth [--from F --to F --target NAME --prop PROP]   set the easing of the keys
  armatura keys     reverse <project.json> <out.json> [--from F --to F --target NAME --prop PROP]   reverse the key order inside the range
  armatura marker   <project.json> <out.json> <name> <frame> [--end F]
  armatura idle     blink <project.json> <out.json> --target T --prop P [--interval S --jitter X --closed F --seed N]   random blinks
  armatura idle     breath <project.json> <out.json> --target T --prop P [--amp A --period F]   slow scale breathing
  armatura idle     sway <project.json> <out.json> --target T --prop P [--amp A --freq HZ --seed N]   gentle noise sway
  armatura follow   <project.json> <out.json> --target SRC --prop P --chain a,b,c [--delay F --decay X]
  armatura import-svg <file.svg> <out.json>            SVG subset -> vector layer (warnings for the rest)
  armatura mocap-info <file.bvh>   hierarchy, channels, frame rate and extent of a BVH file
  armatura mocap-match <project.json> <file.bvh> [--map map.json]   automatic BVH joint to rig bone matching with confidence table
  armatura mocap-retarget <project.json> <file.bvh> <out.json> [--view front|side|top|yaw=DEG] [--map map.json] [--scale auto|N] [--fps F] [--smooth N] [--start F] [--end F] [--rest-frame F] [--min-conf C] [--weight W] [--mirror] [--no-root] [--in-place] [--keep-length] [--footlock BONE --ground Y]   bake mocap as appended channels; a clip longer than the project extends the project length (a message says so), --keep-length cuts the clip at the project length instead
  armatura import-bvh <project.json> <file.bvh> <out.json> --joint-map Joint=bone,.. [--axis Zrotation --tol 0.5]
  armatura face-calibrate <landmarks.ndjson> <out.json> [--calibrate-frames N] [--map mapfile.json] [--fps F] [--layout L]
  armatura face-train --samples N --seed S --out model.bin [--mine-rounds 1] [--scene N] [--reg-win N] [--reg-cell N] [--det-win N] [--det-cell N] [--ridge-lambda X] [--stages N] [--refine-win N] [--stage-patch N] [--stage-cell N] [--stage-lambda X]   train the own face detector on synthetic faces only (no downloads); stylised/cartoon faces, NOT photographs
  armatura face-eval [--model m.bin] [--eval-n 200] [--seed 9] [--scene 128] [--report r.json] [--json]   held-out synthetic accuracy, false positives, robustness sweeps (JSON)
  armatura face-detect <image> [--model m.bin] [--json] [--overlay o.png] [--threshold T] [--max-faces N] [--min-side N] [--max-side N] [--max-dim N]   face boxes and 23 landmarks (armatura23)
  armatura face-track <video-or-frame-dir> <out.ndjson> [--model m.bin] [--fps 30] [--max-dim 256]   landmarks NDJSON for face-bake (video through ffmpeg)
  armatura face-synth <out-dir> [--frames 90] [--size 160] [--seed 7] [--fps 30]   synthetic head animation: PNG frames, truth.ndjson, params.json
  armatura face-bake <project.json> <landmarks.ndjson> <mapfile.json> <out.json> [--calibrate-frames N|0] [--calib calib.json] [--fps F] [--layout L] [--tol T] [--allow-empty]
  armatura lipsync-text <project.json> <audio.wav> <transcript.txt> <switch_layer> <out.json> [--map 0,1,2,3,4] [--lang ro|en] [--words-out timing.json|timing.srt]
  armatura speech-motion <project.json> <audio.wav> <transcript.txt> <out.json> [--head bone] [--brow bone_or_layer] [--nod deg] [--fall deg] [--tilt deg] [--brow-px px] [--seed n] [--lang ro|en] [--words-out timing.json|timing.srt]
  armatura mouth    <project.json> <cues.txt> <switch_layer> <out.json> [--map 0,1,2,3,4] [--rhubarb]

GEOMETRY / RIG / ANIMATION HELPERS
  armatura trace    <image.png> <out.json> [--threshold 128 --tolerance 1.0 --min-area 12 --fill #000000]
  armatura pose-db  build <out.json> [--project P.json] [--library takes.json] [--step N]   pose database (bone-rotation vectors at the key frames of the project and of every library take)
  armatura pose-db  info <db.json>                     bones, sequences, PCA axes and explained variance, library spacing
  armatura pose-db  train-mlp <db.json> <out.json|out.bin> [--hidden 64,64] [--epochs N] [--lr R] [--seed S]   train the optional small pose-completion network (plain SGD) and write the weight file
  armatura pose-suggest between <project.json> <db.json|-> --a F1 --b F2 [--t 0.5] [--snap 0.5] [--top 5] [--apply RANK --out out.json] [--json]   in-between pose of two frames that stays on the learned pose manifold ('-' learns from the project itself)
  armatura pose-suggest complete <project.json> <db.json|-> --frame F [--fix bone=angle,...] [--reach bone=x,y,...] [--free b1,b2] [--prior 1] [--model m.json] [--apply RANK --out out.json]   most plausible full pose given fixed angles and/or tip targets
  armatura pose-suggest next <project.json> <db.json|-> --frame F [--k 8]   what the library did after poses like the one at F
  armatura pose-bench   held-out accuracy of the pose suggestion methods on a synthetic library (also trains the small network)
  armatura autotrace <image.png|jpg> <out.json|out.svg> [--mode fill|line] [--colors N=8] [--min-area A=8] [--tol T=1] [--smooth N=3] [--corner DEG=55] [--denoise N=0] [--quant kmeans|mediancut] [--seed S] [--fit bezier|polygon|vw] [--zorder area|luminance] [--threshold T] [--invert] [--prune N=4] [--into project.json] [--name L] [--fit-stage] [--png preview.png]   local bitmap tracing (no model, no network): colour quantisation, contours, Schneider cubic Bezier fitting, holes by even-odd; line mode thins to centre lines with Zhang-Suen; prints counts and the measured error against the source (mean abs error, SSIM)
  armatura autotrace-bench   trace synthetic shapes / gradient / strokes and print the measured fidelity table
  armatura mesh-trace <project.json> <out.json> <layer> [density=24] [tolerance=1.5] [smooth=N] [hotspot=x,y,r;...] [edge=D/4] [grading=0.5] [lambda=0.5] [taubin=1] [smooth-boundary=0] [threshold=128] [bones=R] [hot-size=E] [skin=1]   traces an image layer's alpha silhouette into a variable-density free triangle mesh (denser at the outline, hot-spots and bones), smooths it and skins it to the bones; prints vertex/triangle counts and quality
  armatura skin-diffuse <project.json> <layer> <out.json> [--influences 4]   heat-diffusion skin weights
  armatura drive    <project.json> <signals.csv> <maps.json> <out.json> [--tol 0.5]   performance -> keys
  armatura state-bake <project.json> <machine.json> <events.json> <out.json> [--tol 0.01]
  armatura pose-match <project.json> <params.json> <goals.json> <out.json> [--generations N --seed N]
  armatura retarget <source.json> <dest.json> <out.json> --pairs srcChain=dstChain[,..]
";

/// Landmark stream plus face config for the `face-*` commands (config from the `face` section of a map file).
fn face_inputs(a: &Args, marks: &str, map: Option<&str>, default_fps: f32) -> Result<(armatura_core::faceparam::LandmarkStream, armatura_core::faceparam::FaceConfig), String> {
    let fps = num(a, "--fps", default_fps)?;
    let layout = a.opts.get("--layout").map(|l| armatura_core::faceparam::Layout::parse(l)).transpose()?;
    let stream = armatura_core::faceparam::parse_ndjson(&fs::read_to_string(marks).map_err(|e| format!("cannot read {marks}: {e}"))?, fps, layout)?;
    for w in &stream.warnings { println!("warning: {w}"); }
    let mf = map.or(a.opts.get("--map").map(String::as_str));
    let cfg = match mf {
        Some(m) => armatura_core::faceparam::parse_mapfile(&fs::read_to_string(m).map_err(|e| format!("cannot read {m}: {e}"))?)?.1,
        None => Default::default(),
    };
    Ok((stream, cfg))
}

fn mocap_map(a: &Args) -> Result<Option<armatura_core::mocap::ManualMap>, String> {
    a.opts.get("--map").map(|f| armatura_core::mocap::parse_map(&fs::read_to_string(f).map_err(|e| format!("cannot read {f}: {e}"))?)).transpose()
}


fn pick_opts(a: &Args) -> Result<armatura_core::pick::PickOptions, String> {
    let mut o = armatura_core::pick::PickOptions::default();
    o.tolerance = num(a, "--tol", o.tolerance)?;
    if !o.tolerance.is_finite() || o.tolerance < 0.0 {
        return Err(format!("--tol needs a number >= 0 (stage units), got '{}'", a.opts.get("--tol").map(String::as_str).unwrap_or("")));
    }
    o.bones = a.flags.iter().any(|f| f == "--bones");
    if a.flags.iter().any(|f| f == "--root") {
        o.group_mode = armatura_core::pick::GroupMode::Root;
    }
    let list = |k: &str| a.opts.get(k).map(|v| v.split(',').filter(|s| !s.is_empty()).map(String::from).collect::<Vec<_>>()).unwrap_or_default();
    o.point_layers = list("--points");
    o.locked = list("--locked");
    Ok(o)
}

fn describe_target(t: &armatura_core::pick::Target) -> String {
    use armatura_core::pick::Target;
    match t {
        Target::Layer { path } => format!("layer {path}"),
        Target::Bone { name } => format!("bone {name}"),
        Target::Point { layer, point } => format!("point {} of {layer}", serde_json::to_string(point).unwrap_or_default()),
    }
}

fn timeline_text(m: &armatura_core::timeline::TimelineModel) -> String {
    use armatura_core::timeline::Row;
    let mut s = format!("timeline: {} frames at {} fps, {} channels\n", m.frames, m.fps, m.tracks.len());
    for r in &m.rows {
        match *r {
            Row::Group(g) => {
                let g = &m.groups[g];
                s.push_str(&format!("{}{}{}\n", "  ".repeat(g.depth), g.label, if g.locked { " [locked]" } else { "" }));
            }
            Row::Track(t) => {
                let t = &m.tracks[t];
                let depth = m.groups[t.group].depth + 1;
                let keys: Vec<String> = t.keys.iter().map(|k| format!("{}", k.frame)).collect();
                let mut flags = String::new();
                if t.locked { flags.push_str(" [locked]"); }
                if t.shy { flags.push_str(" [shy]"); }
                if t.solo { flags.push_str(" [solo]"); }
                if let Some(c) = &t.color { flags.push_str(&format!(" {c}")); }
                let body = if t.source == "keys" { format!("keys @ {}", keys.join(" ")) } else { t.source.to_string() };
                s.push_str(&format!("{}#{} {} ({}){}\n", "  ".repeat(depth), t.ch, t.label, body, flags));
            }
        }
    }
    s
}

/// `TARGET:PROP[@F0..F1]` items joined by `;`, `#CH[@F0..F1]`, or `all`.
fn parse_key_sel(m: &armatura_core::timeline::TimelineModel, spec: &str) -> Result<Vec<armatura_core::timeline::KeyRef>, String> {
    use armatura_core::timeline::{normalize, KeyRef};
    let mut out: Vec<KeyRef> = Vec::new();
    if spec == "all" {
        return Ok(armatura_core::timeline::select_all(m, false));
    }
    for item in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let (head, range) = match item.split_once('@') {
            None => (item, None),
            Some((h, r)) => {
                let (a, b) = r.split_once("..").ok_or_else(|| format!("bad range '{r}' (use F0..F1)"))?;
                let f = |x: &str| x.trim().parse::<f32>().map_err(|_| format!("bad frame '{x}' in range"));
                (h, Some((f(a)?, f(b)?)))
            }
        };
        let chans: Vec<usize> = if let Some(n) = head.strip_prefix('#') {
            vec![n.parse::<usize>().map_err(|_| format!("bad channel '{n}'"))?]
        } else {
            let (target, prop) = head.rsplit_once(':').ok_or_else(|| format!("selector '{head}' needs TARGET:PROP"))?;
            let prop = Prop::parse(prop).ok_or_else(|| format!("unknown property '{prop}'"))?;
            m.tracks.iter().filter(|t| t.target == target && t.prop == prop && t.editable).map(|t| t.ch).collect()
        };
        if chans.is_empty() {
            return Err(format!("nothing matches '{head}'"));
        }
        let (lo, hi) = range.unwrap_or((f32::NEG_INFINITY, f32::INFINITY));
        let mut any = false;
        for c in chans {
            let t = m.tracks.get(c).ok_or_else(|| format!("no channel {c}"))?;
            for k in t.keys.iter().filter(|k| k.frame >= lo.min(hi) && k.frame <= lo.max(hi)) {
                out.push(KeyRef { ch: c, frame: k.frame });
                any = true;
            }
        }
        if !any {
            return Err(format!("no keys in '{item}'"));
        }
    }
    normalize(&mut out);
    if out.is_empty() {
        return Err("empty selection".into());
    }
    Ok(out)
}

fn parse_ease_arg(s: &str) -> Result<armatura_core::anim::Ease, String> {
    use armatura_core::anim::Ease;
    if s.trim_start().starts_with('{') {
        return serde_json::from_str(s).map_err(|e| format!("bad ease JSON: {e}"));
    }
    if let Some(r) = s.strip_prefix("bezier:") {
        let n: Vec<f32> = r.split(',').map(|x| x.trim().parse::<f32>().map_err(|_| format!("bad number '{x}'"))).collect::<Result<_, _>>()?;
        if n.len() != 4 {
            return Err("bezier needs x1,y1,x2,y2".into());
        }
        return Ok(Ease::CubicBezier { x1: n[0], y1: n[1], x2: n[2], y2: n[3] });
    }
    serde_json::from_value(serde_json::json!({"type": s})).map_err(|_| format!("unknown ease '{s}'"))
}

fn parse_list_usize(s: &str) -> Result<Vec<usize>, String> {
    s.split(',').filter(|x| !x.is_empty()).map(|x| x.trim().parse::<usize>().map_err(|_| format!("bad channel index '{x}'"))).collect()
}

fn keyedit_cmd(a: &Args, path: &str, out: &str, sub: &str) -> Result<(), String> {
    use armatura_core::timeline::{self as tl, Clip, Collision, EditOpts, Remap, RetimeMode, TlOp};
    use armatura_core::timeline_graph::{HandleMode, Side, TangentKind};
    let p = load_project(path)?;
    let m = tl::build(&p);
    let mut ed = Editor::with_asset_dir(p, asset_dir(path)).map_err(|errs| format!("{path} is not a valid project:\n  - {}", errs.join("\n  - ")))?;
    let flag = |f: &str| a.flags.iter().any(|x| x == f);
    let opt = |k: &str| a.opts.get(k).map(String::as_str);
    let need = |k: &str| opt(k).ok_or_else(|| format!("keyedit {sub} needs {k}"));
    let mut opts = EditOpts { snap: !flag("--no-snap"), ..EditOpts::default() };
    if flag("--free") {
        opts.bounds = tl::Bounds::Free;
    }
    if let Some(c) = opt("--collision") {
        opts.collision = Collision::parse(c).ok_or_else(|| format!("--collision must be replace, merge or refuse, got '{c}'"))?;
    }
    let sel = || -> Result<Vec<tl::KeyRef>, String> { parse_key_sel(&m, need("--sel")?) };
    let n = |k: &str, d: f32| num(a, k, d);
    let op = match sub {
        "move" => TlOp::MoveKeys { keys: sel()?, dt: n("--dt", 0.0)?, dv: n("--dv", 0.0)?, opts },
        "scale" => TlOp::ScaleKeys { keys: sel()?, pivot: n("--pivot", 0.0)?, factor: n("--factor", 1.0)?, opts },
        "dup" => TlOp::Duplicate { keys: sel()?, offset: n("--offset", f32::NAN)?, opts },
        "delete" => TlOp::Delete { keys: sel()? },
        "ease" => TlOp::SetEase { keys: sel()?, ease: parse_ease_arg(need("--ease")?)? },
        "value" => TlOp::SetValue { keys: sel()?, value: n("--value", f32::NAN)? },
        "smooth" => TlOp::Smooth {
            keys: sel()?,
            kind: match opt("--kind").unwrap_or("monotone") {
                "monotone" => TangentKind::Monotone,
                "catmull_rom" => TangentKind::CatmullRom,
                k => return Err(format!("--kind must be monotone or catmull_rom, got '{k}'")),
            },
        },
        "copy" => TlOp::Copy { keys: sel()? },
        "paste" => {
            let f = need("--clip")?;
            let clip: Clip = serde_json::from_str(&fs::read_to_string(f).map_err(|e| format!("cannot read {f}: {e}"))?).map_err(|e| format!("bad clip {f}: {e}"))?;
            let mut remap = Vec::new();
            if let Some(r) = opt("--remap") {
                let (from, to) = r.split_once('=').ok_or("--remap needs from_target:prop=to_target:prop")?;
                let (ft, fp) = from.rsplit_once(':').ok_or("--remap source needs target:prop")?;
                let (tt, tp) = to.rsplit_once(':').ok_or("--remap destination needs target:prop")?;
                remap.push(Remap { from_target: ft.into(), from_prop: Prop::parse(fp).ok_or_else(|| format!("unknown property '{fp}'"))?, to_target: tt.into(), to_prop: Prop::parse(tp).ok_or_else(|| format!("unknown property '{tp}'"))? });
            }
            TlOp::Paste { clip, at: n("--at", 0.0)?, remap, opts }
        }
        "add" => TlOp::AddKey {
            target: need("--target")?.to_string(),
            prop: Prop::parse(need("--prop")?).ok_or("unknown --prop")?,
            frame: n("--at", 0.0)?,
            value: opt("--value").map(|_| n("--value", 0.0)).transpose()?,
            ease: opt("--ease").map(parse_ease_arg).transpose()?,
            opts,
        },
        "ripple" => TlOp::Ripple {
            at: n("--at", 0.0)?,
            count: n("--by", f32::NAN)?,
            mode: match opt("--mode").unwrap_or("insert") {
                "insert" => RetimeMode::Insert,
                "delete" => RetimeMode::Delete,
                x => return Err(format!("--mode must be insert or delete, got '{x}'")),
            },
            scope: opt("--scope").map(parse_list_usize).transpose()?,
            adjust_frames: flag("--adjust-frames"),
            opts,
        },
        "offset" => TlOp::Offset { channels: parse_list_usize(need("--channels")?)?, dt: n("--dt", f32::NAN)?, opts },
        "extrapolate" => TlOp::Extrapolate {
            channels: parse_list_usize(need("--channels")?)?,
            extrapolate: serde_json::from_value(serde_json::Value::String(need("--extrapolate")?.into())).map_err(|_| "--extrapolate must be hold, linear, loop, ping_pong or loop_offset")?,
        },
        "loop" => TlOp::MakeLoop { channels: parse_list_usize(need("--channels")?)?, end: opt("--end").map(|_| n("--end", 0.0)).transpose()?, extrapolate: None, opts },
        "handle" => {
            let s = sel()?;
            if s.len() != 1 {
                return Err("handle needs --sel to match exactly one key".into());
            }
            TlOp::Handle {
                ch: s[0].ch,
                frame: s[0].frame,
                side: match need("--side")? {
                    "in" => Side::In,
                    "out" => Side::Out,
                    x => return Err(format!("--side must be in or out, got '{x}'")),
                },
                t: n("--t", f32::NAN)?,
                v: n("--v", f32::NAN)?,
                mode: match opt("--mode").unwrap_or("free") {
                    "free" => HandleMode::Free,
                    "aligned" => HandleMode::Aligned,
                    "auto" => HandleMode::Auto,
                    x => return Err(format!("--mode must be free, aligned or auto, got '{x}'")),
                },
            }
        }
        "lock" => {
            let on = !matches!(opt("--value"), Some("off") | Some("false") | Some("0"));
            TlOp::Flags { target: need("--target")?.to_string(), prop: opt("--prop").map(|x| Prop::parse(x).ok_or("unknown --prop")).transpose()?, lock: Some(on), shy: None, solo: None, color: None }
        }
        x => return Err(format!("unknown keyedit subcommand '{x}' (move, scale, dup, delete, ease, value, smooth, copy, paste, add, ripple, offset, extrapolate, loop, handle, lock)")),
    };
    let plan = tl::plan(ed.project(), &op)?;
    if let (Some(clip), Some(f)) = (&plan.clip, opt("--clip")) {
        fwrite(f, serde_json::to_string_pretty(clip).map_err(|e| e.to_string())?)?;
    }
    if out == "-" {
        println!("{}", serde_json::to_string_pretty(&plan).map_err(|e| e.to_string())?);
        return Ok(());
    }
    if let Some(o) = &plan.op {
        ed.apply(o.clone()).map_err(|e| e.join("; "))?;
        fwrite(out, serde_json::to_string_pretty(ed.doc()).map_err(|e| e.to_string())?)?;
    }
    println!("{sub}: {} keys changed{}{}", plan.changed, if plan.op.is_some() { format!(", wrote {out}") } else { ", project unchanged".into() }, if plan.clip.is_some() { ", clip saved" } else { "" });
    for note in &plan.notes {
        println!("  note: {note}");
    }
    Ok(())
}

/// `unknown field <path>` lines for keys of the project file that the model ignores (typos, newer versions).
fn unknown_field_warnings(path: &str) -> Vec<String> {
    match read_text_capped(path) {
        Ok(t) => armatura_core::unknownfields::unknown_fields(&t).unwrap_or_default().into_iter().map(|f| format!("unknown field {f}")).collect(),
        Err(_) => Vec::new(),
    }
}

/// Largest project file `load_project` reads (override with `--max-input-bytes N`).
static MAX_INPUT_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(256 << 20);

/// Reads a project file as text: a regular file only (never a device or FIFO such as /dev/zero) and at most
/// `MAX_INPUT_BYTES` (also enforced while reading, in case the file grows).
fn read_text_capped(path: &str) -> Result<String, String> {
    use std::io::Read;
    let max = MAX_INPUT_BYTES.load(std::sync::atomic::Ordering::SeqCst);
    let md = fs::metadata(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    if !md.is_file() {
        return Err(format!("cannot read {path}: not a regular file (devices, pipes and directories are refused)"));
    }
    if md.len() > max {
        return Err(format!("cannot read {path}: {} bytes exceed the input limit of {max} bytes (raise it with --max-input-bytes N)", md.len()));
    }
    let f = fs::File::open(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut buf = Vec::new();
    f.take(max + 1).read_to_end(&mut buf).map_err(|e| format!("cannot read {path}: {e}"))?;
    if buf.len() as u64 > max {
        return Err(format!("cannot read {path}: larger than the input limit of {max} bytes (raise it with --max-input-bytes N)"));
    }
    String::from_utf8(buf).map_err(|_| format!("cannot read {path}: stream did not contain valid UTF-8"))
}

fn load_project(path: &str) -> Result<Project, String> {
    let text = read_text_capped(path)?;
    let p = Project::from_json(&text).map_err(|e| format!("invalid JSON in {path}: {e}"))?;
    // relative assets of every derived project (validation in write_json, lod, dof, ...) are looked up next to the input
    armatura_core::assets::set_default_dir(Some(asset_dir(path)));
    Ok(p)
}

/// The overall verdict of `collab-sim`: convergence alone is not enough, the merged document must
/// also compile as a project.
fn collab_ok(converged_ok: bool, merged_compiles: bool) -> bool {
    converged_ok && merged_compiles
}

/// `armatura collab-sim`: random concurrent editing by N replicas (see `armatura_core::collab`).
fn collab_sim(a: &Args, path: &str) -> Result<(), String> {
    use armatura_core::collab::{simulate, Replica, SimConfig, SimRng};
    let p = load_project(path)?;
    let doc = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    let whole = |k: &str, d: usize, lo: usize, hi: usize| -> Result<usize, String> {
        let v = num(a, k, d as f32)?;
        if v < lo as f32 || v > hi as f32 || v.fract() != 0.0 {
            return Err(format!("{k} needs a whole number in {lo}..={hi}"));
        }
        Ok(v as usize)
    };
    let mut cfg = SimConfig::new(whole("--clients", 3, 1, 64)?, whole("--ops", 100, 1, 200_000)?, a.opts.get("--seed").map(|s| s.parse::<u64>().map_err(|_| "--seed needs a whole number".to_string())).transpose()?.unwrap_or(1));
    cfg.undo_pct = whole("--undo-pct", 12, 0, 90)?;
    let raw = a.flags.iter().any(|f| f == "--raw");
    cfg.project_aware = !raw;
    let replays = whole("--replays", 10, 0, 1000)?;
    let rep = simulate(&doc, &cfg)?;
    // the same envelopes, delivered to fresh replicas in other random orders, must give the same state
    let mut replay_ok = 0;
    let mut first_bad = None;
    for k in 0..replays {
        let mut order = rep.trace.clone();
        SimRng(cfg.seed ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9)).shuffle(&mut order);
        let mut r = Replica::new(1000 + k as u32, &doc)?;
        let mut bad = false;
        for e in order {
            if r.receive(e).is_err() {
                bad = true;
                break;
            }
        }
        if !bad && r.pending_len() == 0 && r.json() == rep.final_doc {
            replay_ok += 1;
        } else if first_bad.is_none() {
            first_bad = Some(k);
        }
    }
    // informative only: does the merged result still compile as a project? (random edits are arbitrary)
    let valid = serde_json::from_value::<Project>(rep.final_doc.clone()).ok().map(|pr| Engine::new(pr).is_ok()).unwrap_or(false);
    let converged_ok = rep.converged && rep.state_identical && rep.clocks_identical && rep.pending_left == 0 && replay_ok == replays;
    let ok = collab_ok(converged_ok, valid);
    let strict = a.flags.iter().any(|f| f == "--require-valid");
    if a.flags.iter().any(|f| f == "--json") {
        println!("{}", serde_json::json!({
            "ok": ok, "converged_ok": converged_ok, "clients": rep.clients, "seed": cfg.seed, "edits": rep.edits, "envelopes": rep.envelopes, "noops": rep.noops, "rejected_local": rep.rejected_local,
            "undo_redo_steps": rep.undos, "deliveries": rep.deliveries, "duplicates_dropped": rep.duplicates, "buffered": rep.buffered, "dropped_prims": rep.dropped_prims,
            "converged": rep.converged, "state_identical": rep.state_identical, "clocks_identical": rep.clocks_identical, "pending_left": rep.pending_left,
            "replays": replays, "replays_identical": replay_ok, "tombstones": rep.tombstones, "final_json_bytes": rep.doc_bytes, "merged_project_compiles": valid,
        }));
    } else {
        println!("{} replicas, {} edits ({} envelopes, {} no-ops, {} rejected locally, {} undo/redo steps), seed {}", rep.clients, rep.edits, rep.envelopes, rep.noops, rep.rejected_local, rep.undos, cfg.seed);
        println!("deliveries {} (duplicates ignored {}, held back by the causal buffer {}), primitives dropped {}", rep.deliveries, rep.duplicates, rep.buffered, rep.dropped_prims);
        println!("visible documents identical: {}; internal states identical: {}; vector clocks identical: {}; envelopes still waiting: {}", rep.converged, rep.state_identical, rep.clocks_identical, rep.pending_left);
        println!("replays of the same envelopes in other random orders: {replay_ok}/{replays} identical");
        println!("merged document: {} bytes, {} tombstones; compiles as a project: {} (informative with --raw: those edits are arbitrary JSON)", rep.doc_bytes, rep.tombstones, valid);
        println!("{}", if converged_ok { "CONVERGED" } else { "DIVERGED" });
    }
    if converged_ok && (ok || !strict) {
        Ok(())
    } else if converged_ok {
        Err("replicas converged, but the merged document does not compile as a project".into())
    } else {
        Err(rep.divergence.unwrap_or_else(|| format!("replay {} differs", first_bad.unwrap_or(0))))
    }
}

/// `pattern` or `pattern:prop` (prop in snake_case, e.g. bone_angle).
fn parse_pattern_prop(s: &str) -> Result<(String, Option<armatura_core::project::Prop>), String> {
    match s.split_once(':') {
        None => Ok((s.to_string(), None)),
        Some((pat, pr)) => {
            let prop = serde_json::from_value(serde_json::Value::String(pr.to_string())).map_err(|_| format!("unknown property '{pr}'"))?;
            Ok((pat.to_string(), Some(prop)))
        }
    }
}

fn asset_dir(path: &str) -> std::path::PathBuf {
    Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf()).unwrap_or_else(|| ".".into())
}

fn write_words(a: &Args, al: &speech::Alignment) -> Result<(), String> {
    if let Some(f) = a.opts.get("--words-out") {
        let body = if f.to_ascii_lowercase().ends_with(".srt") { speech::words_srt(al) } else { speech::words_json(al)? };
        fwrite(f, body)?;
    }
    Ok(())
}

fn compile(project: Project, path: &str) -> Result<Engine, String> {
    Engine::with_asset_dir(project, &asset_dir(path))
        .map_err(|errs| format!("{path} is not a valid project:\n  - {}", errs.join("\n  - ")))
}

fn load(path: &str) -> Result<Engine, String> {
    compile(load_project(path)?, path)
}

/// Like `load`, but honours `--size WxH`: layout containers re-solve for that canvas size (the file is not changed).
fn load_sized(path: &str, a: &Args) -> Result<Engine, String> {
    let mut p = load_project(path)?;
    if let Some(sz) = a.opts.get("--size") {
        let (w, h) = armatura_core::containers::parse_size(sz)?;
        p = armatura_core::containers::responsive(&p, w, h)?;
    }
    compile(p, path)
}

/// Positional arguments, bare flags, and `--key value` options.
struct Args {
    pos: Vec<String>,
    flags: Vec<String>,
    opts: HashMap<String, String>,
    scale: f32,
}

const VALUE_OPTS: &[&str] = &[
    "--map", "--thigh-l", "--thigh-r", "--shin-l", "--shin-r", "--arm-l", "--arm-r", "--hips", "--period", "--stride",
    "--knee", "--bob", "--swing", "--port", "--scale", "--columns", "--threshold", "--tolerance", "--min-area", "--fill",
    "--factor", "--influences", "--codec", "--pairs", "--tol", "--fps", "--frames", "--generations", "--seed",
    "--from", "--to", "--by", "--pivot", "--ease", "--target", "--prop", "--end", "--axis", "--window", "--interval",
    "--amp", "--period", "--freq", "--decay", "--delay", "--closed", "--jitter", "--chain", "--joint-map",
    "--trim-in", "--trim-out", "--fade-in", "--fade-out", "--blend", "--focus", "--strength", "--fit", "--max-radius", "--samples", "--start", "--keep", "--number", "--gain", "--data", "--takes", "--precision", "--at", "--speed", "--loops", "--mode", "--weight", "--include", "--exclude", "--track-weight", "--rename", "--keys", "--track", "--lang", "--words-out", "--head", "--brow", "--nod", "--fall", "--tilt", "--brow-px", "--calibrate-frames", "--calib", "--layout", "--view", "--smooth", "--rest-frame", "--footlock", "--ground", "--min-conf", "--expr", "--remap", "--svg-frames", "--frame", "--x", "--y", "--rect", "--lasso", "--points", "--locked", "--size", "--offset", "--velocity", "--time", "--scroll", "--sel", "--dt", "--dv", "--value", "--collision", "--clip", "--side", "--t", "--v", "--kind", "--channels", "--extrapolate", "--scope", "--ui-config", "--before", "--after", "--step", "--proxy", "--tint-before", "--tint-after", "--opacity", "--falloff", "--layers", "--adapter", "--bench", "--clients", "--ops", "--bind", "--allow-host", "--public-origin", "--source-url", "--collab-token", "--collab-log", "--replays", "--undo-pct",
    "--a", "--b", "--snap", "--top", "--fix", "--reach", "--free", "--hidden", "--epochs", "--lr", "--library", "--project", "--model", "--prior", "--out", "--apply", "--k",
    "--strategy", "--report", "--heatmap", "--db", "--res", "--max-fail-rate", "--props", "--margin", "--only", "--ignore",
    "--max-size", "--max-seconds", "--format", "--tolerance-px", "--levels", "--out-dir", "--screen-px", "--max-px", "--ratio", "--measure", "--min-ssim", "--pareto", "--search", "--colors", "--corner", "-m", "--message", "--collab-secret-file", "--collab-revoke", "--collab-rate", "--secret-file", "--ttl", "--author", "--repo", "--limit", "-d", "--prune", "--max-input-bytes", "--junit", "--html", "--artifacts", "--metric", "--overlay", "--flicker", "--pixel-de", "--aa", "--denoise", "--fit", "--quant", "--zorder", "--into", "--name", "--scene",
    "--min-side", "--max-side", "--max-faces", "--max-dim", "--eval-n", "--mine-rounds", "--overlay", "--reg-win", "--reg-cell", "--det-win", "--det-cell", "--ridge-lambda", "--stages", "--refine-win", "--stage-patch", "--stage-cell", "--stage-lambda",
];

/// Options whose kind differs from the global table for ONE command: `(command, option, takes_value)`.
/// `VALUE_OPTS` lists the options that take a value in most commands; this table is where a command differs
/// (a boolean `--prune` for `ci run` but `--prune N` for `autotrace`, `--png preview.png` only for `autotrace`).
/// Nothing else special-cases an option in the parser, and a test checks every USAGE entry against it.
const CMD_OPTS: &[(&str, &str, bool)] = &[
    ("ci", "--prune", false),
    ("export-oca", "--layers", false),
    ("keyedit", "--free", false),
    ("autotrace", "--png", true),
];

/// Does `opt` take a value in command `cmd`?
fn takes_value(cmd: &str, opt: &str) -> bool {
    if cmd == "ci" {
        return ci_cmd::is_value_opt(opt); // `ci` has its own table (its subcommands share options)
    }
    if let Some((_, _, v)) = CMD_OPTS.iter().find(|(c, o, _)| *c == cmd && *o == opt) {
        return *v;
    }
    VALUE_OPTS.contains(&opt)
}

/// Rewrites `--key=value` to `--key value` for value options of the command; a `--flag=x` for a boolean flag is
/// refused with the right hint. Unknown `--x=y` words are left alone (they may be positional data).
fn expand_eq(raw: &[String]) -> Result<Vec<String>, String> {
    let cmd = raw.first().filter(|c| !c.starts_with('-')).map(String::as_str).unwrap_or("");
    let known = documented_options(cmd);
    let mut out = Vec::with_capacity(raw.len());
    let mut skip_value = false;
    for (i, a) in raw.iter().enumerate() {
        if skip_value {
            skip_value = false;
            out.push(a.clone());
            continue;
        }
        if i > 0 && a.starts_with("--") && a.len() > 2 {
            if let Some((k, v)) = a.split_once('=') {
                if takes_value(cmd, k) {
                    out.push(k.to_string());
                    out.push(v.to_string());
                    continue;
                }
                if known.iter().any(|o| o == k) || GLOBAL_FLAGS.contains(&k) || (cmd == "ci" && ci_cmd::is_flag(k)) {
                    return Err(format!("{k} is a flag and takes no value: write {k} (without =), got '{a}'"));
                }
            } else if takes_value(cmd, a) {
                skip_value = true; // the next word is this option's value, even if it contains '='
            }
        }
        out.push(a.clone());
    }
    Ok(out)
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut pos, mut flags, mut opts, mut scale) = (Vec::new(), Vec::new(), HashMap::new(), 1.0f32);
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if takes_value(cmd, a) {
            i += 1;
            let v = args.get(i).ok_or_else(|| format!("{a} needs a value"))?.clone();
            if a == "--scale" && !args.iter().any(|x| x == "mocap-retarget") {
                scale = v.parse().ok().filter(|s: &f32| *s > 0.0 && *s <= 16.0).ok_or("--scale needs a number in (0, 16]")?;
            }
            opts.insert(a.to_string(), v);
        } else if a.starts_with("--") {
            flags.push(a.to_string());
        } else {
            pos.push(a.to_string());
        }
        i += 1;
    }
    Ok(Args { pos, flags, opts, scale })
}

/// Flags every command accepts.
/// (`--json`, `--quiet` and `--allow-empty` are per-command options: a command that does not list them in its usage refuses them.)
const GLOBAL_FLAGS: &[&str] = &["--version", "--help", "--follow-symlinks", "--allow-external-assets"];


/// The USAGE text of one command (its line and continuation lines), for `armatura <command> --help`.
fn usage_for(cmd: &str) -> Option<String> {
    let mut out = String::new();
    let mut inside = false;
    for l in USAGE.lines() {
        if l.starts_with("  armatura ") {
            inside = l.split_whitespace().nth(1) == Some(cmd);
        } else if !l.starts_with("      ") {
            inside = false;
        }
        if inside {
            out.push_str(l);
            out.push('\n');
        }
    }
    if out.is_empty() { None } else { Some(format!("USAGE:\n{out}")) }
}

/// One synopsis form of a command: a USAGE line, or one `|`-separated alternative of a line
/// (`ci init ... | run ... | diff ...`, `keyedit ... scale ... | dup ...`).
#[derive(Debug, Clone)]
struct UsageForm {
    cmd: String,
    /// The sub-command word(s) that select this form (`init`, `commit|revert`); empty for a form without one.
    keys: Vec<String>,
    /// Options written outside `[...]`: the form is only meant when these are present (`trail ... --target X`).
    required: Vec<String>,
    /// Every `--option` of the form, including the ones named in its continuation / description text.
    opts: Vec<String>,
    /// Output extensions the form is for (`<out.gif>`, `<out.json|out.lottie>`): `export` picks its form by the extension.
    exts: Vec<String>,
}

/// The extensions named by `out.<ext>` words of a synopsis (`<out.json|out.lottie>` gives `json`, `lottie`).
fn form_exts(text: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for w in text.split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '|' || c == '[' || c == ']') {
        if let Some(e) = w.strip_prefix("out.") {
            if !e.is_empty() && e.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && !v.iter().any(|x| x == e) {
                v.push(e.to_string());
            }
        }
    }
    v
}

/// `--names` found in `text` (alphanumerics and '-'); with `top_only`, only outside `[...]`.
fn option_names(text: &str, top_only: bool) -> Vec<String> {
    let (mut out, mut depth) = (Vec::<String>::new(), 0i32);
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '[' => depth += 1,
            ']' => depth = (depth - 1).max(0),
            '-' if i + 1 < b.len() && b[i + 1] == '-' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == '-')) && (!top_only || depth == 0) => {
                let mut j = i + 2;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '-') {
                    j += 1;
                }
                let name: String = b[i..j].iter().collect();
                if name.len() > 2 && !out.contains(&name) {
                    out.push(name);
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// Splits at ` | ` (a pipe with white space on both sides) outside `[...]`, `<...>` and `(...)`.
fn split_alternatives(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    let b: Vec<char> = s.chars().collect();
    for (i, &c) in b.iter().enumerate() {
        match c {
            '[' | '<' | '(' => depth += 1,
            ']' | '>' | ')' => depth = (depth - 1).max(0),
            _ => {}
        }
        if c == '|' && depth == 0 && i > 0 && b[i - 1].is_whitespace() && b.get(i + 1).is_some_and(|n| n.is_whitespace()) {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out.into_iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// The sub-command word(s) a synopsis starts with: the first bare lower-case word before the first option or `[...]`
/// (`<placeholders>` are skipped), e.g. `init`, `commit|revert`.
fn form_keys(text: &str) -> Vec<String> {
    for t in text.split_whitespace() {
        if t.starts_with('[') || t.starts_with('-') {
            break;
        }
        if t.starts_with('<') {
            continue;
        }
        let ok = t.split('|').all(|w| !w.is_empty() && w.starts_with(|c: char| c.is_ascii_lowercase()) && w.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        if ok {
            return t.split('|').map(String::from).collect();
        }
    }
    Vec::new()
}

/// Every synopsis form of every command in USAGE (parsed once).
fn usage_forms() -> &'static Vec<UsageForm> {
    static FORMS: std::sync::OnceLock<Vec<UsageForm>> = std::sync::OnceLock::new();
    FORMS.get_or_init(|| {
        // (command, body of the first line, description of the first line, continuation lines)
        let mut lines: Vec<(String, String, String, Vec<String>)> = Vec::new();
        for l in USAGE.lines() {
            if let Some(rest) = l.strip_prefix("  armatura ") {
                let rest = rest.trim_start();
                let cmd = rest.split_whitespace().next().unwrap_or("").to_string();
                let tail = rest[cmd.len()..].trim_start();
                let (body, desc) = match tail.find("   ") {
                    Some(i) => (tail[..i].to_string(), tail[i..].trim().to_string()),
                    None => (tail.to_string(), String::new()),
                };
                lines.push((cmd, body, desc, Vec::new()));
            } else if l.starts_with("      ") {
                if let Some(last) = lines.last_mut() {
                    last.3.push(l.trim().to_string());
                }
            }
        }
        let mut forms = Vec::new();
        for (cmd, body, desc, cont) in lines {
            if cmd.starts_with('-') {
                continue;
            }
            let mut segs: Vec<(String, String)> = split_alternatives(&body)
                .into_iter()
                .map(|seg| {
                    let t0 = seg.split_whitespace().next().unwrap_or("");
                    if t0 == cmd || (t0.starts_with(&format!("{cmd}-")) && !t0.contains('<')) {
                        (t0.to_string(), seg[t0.len()..].trim().to_string())
                    } else {
                        (cmd.clone(), seg)
                    }
                })
                .collect();
            if segs.is_empty() {
                segs.push((cmd.clone(), String::new()));
            }
            // the prose after the gap on the first line describes the command; only its synopsis and continuation
            // lines declare options (a `--features gpu` mentioned in prose is not an option of the command)
            let _ = desc;
            // continuation lines belong to the alternatives that name them ("trim/fade options"), else to the last one
            let cont = cont.join(" ");
            let shared: Vec<usize> = (0..segs.len()).filter(|&i| segs[i].1.contains("trim/fade options")).collect();
            let last = segs.len() - 1;
            let mut tails = vec![String::new(); segs.len()];
            for i in if shared.is_empty() { vec![last] } else { shared } {
                tails[i].push(' ');
                tails[i].push_str(&cont);
            }
            for ((c, text), tail) in segs.into_iter().zip(tails) {
                forms.push(UsageForm { cmd: c, keys: form_keys(&text), required: option_names(&text, true), opts: option_names(&format!("{text} {tail}"), false), exts: form_exts(&text) });
            }
        }
        forms
    })
}

/// Every `--option` mentioned in the USAGE block(s) of `cmd` (documented flags and value options), all forms pooled.
fn documented_options(cmd: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in usage_forms().iter().filter(|f| f.cmd == cmd) {
        for o in &f.opts {
            if !out.contains(o) {
                out.push(o.clone());
            }
        }
    }
    out
}

/// The synopsis forms that apply to one invocation: the forms whose sub-command word is among the positionals
/// (else the forms without a sub-command word, else all); among several, those whose required options are present.
fn select_forms<'a>(cmd: &str, pos: &[String], present: &[String]) -> Vec<&'a UsageForm> {
    let all: Vec<&UsageForm> = usage_forms().iter().filter(|f| f.cmd == cmd).collect();
    if all.len() <= 1 {
        return all;
    }
    // forms that differ by the output extension (`export`): the form for the extension of the output file
    if all.iter().all(|f| !f.exts.is_empty() && f.keys.is_empty()) {
        let ext = pos.get(2).and_then(|o| Path::new(o).extension()).and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        let by_ext: Vec<&UsageForm> = all.iter().copied().filter(|f| f.exts.contains(&ext)).collect();
        if !by_ext.is_empty() && by_ext.len() < all.len() {
            return by_ext;
        }
    }
    let keyed: Vec<&UsageForm> = all.iter().copied().filter(|f| f.keys.iter().any(|k| pos.iter().skip(1).any(|p| p == k))).collect();
    let mut cand = if !keyed.is_empty() {
        keyed
    } else {
        let keyless: Vec<&UsageForm> = all.iter().copied().filter(|f| f.keys.is_empty()).collect();
        if keyless.is_empty() { all } else { keyless }
    };
    if cand.len() > 1 {
        let ok: Vec<&UsageForm> = cand.iter().copied().filter(|f| f.required.iter().all(|r| present.contains(r))).collect();
        if let Some(m) = ok.iter().map(|f| f.required.len()).max() {
            cand = ok.into_iter().filter(|f| f.required.len() == m).collect();
        }
    }
    cand
}

/// Lower-case extension of the output file (third positional) of a command whose forms differ by it.
fn pos_ext(pos: &[String]) -> String {
    pos.get(2).and_then(|o| Path::new(o).extension()).and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

/// Options every command accepts besides [`GLOBAL_FLAGS`].
const GLOBAL_OPTS: &[&str] = &["--max-input-bytes"];

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1])));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Refuse an option (flag or value option) that the sub-form of the command does not list, with a "did you mean" hint
/// instead of silently ignoring it. Validation is per synopsis form: `ci diff a.png b.png --quiet` is refused although
/// `ci run` documents `--quiet`.
fn check_flags(a: &Args, raw: &[String]) -> Result<(), String> {
    let Some(cmd) = raw.first().filter(|c| !c.starts_with('-')) else { return Ok(()) };
    let pooled = documented_options(cmd);
    if pooled.is_empty() && usage_for(cmd).is_none() {
        return Ok(()); // not a command with a USAGE block (the dispatcher reports it)
    }
    let mut present: Vec<String> = a.flags.clone();
    present.extend(a.opts.keys().cloned());
    let forms = select_forms(cmd, &a.pos, &present);
    let mut known: Vec<&str> = Vec::new();
    for f in &forms {
        known.extend(f.opts.iter().map(String::as_str));
    }
    let label = if forms.len() == 1 && !forms[0].exts.is_empty() && usage_forms().iter().filter(|u| u.cmd == *cmd).count() > 1 && forms[0].keys.is_empty() {
        format!("{cmd} .{}", pos_ext(&a.pos))
    } else {
        let key = forms.first().filter(|_| forms.len() == 1).and_then(|f| f.keys.iter().find(|k| a.pos.iter().skip(1).any(|p| p == *k)));
        match key {
            Some(k) => format!("{cmd} {k}"),
            None => cmd.clone(),
        }
    };
    let mut given: Vec<&String> = a.flags.iter().collect();
    given.extend(a.opts.keys().filter(|k| k.starts_with("--")));
    given.sort();
    for f in given {
        if known.contains(&f.as_str()) || GLOBAL_FLAGS.contains(&f.as_str()) || GLOBAL_OPTS.contains(&f.as_str()) {
            continue;
        }
        let best = known.iter().copied().chain(GLOBAL_FLAGS.iter().copied()).map(|k| (edit_distance(f, k), k)).min();
        let mut hint = match best {
            Some((d, k)) if d <= 3 || k.starts_with(f.as_str()) => format!(" (did you mean {k}?)"),
            _ => String::new(),
        };
        let owners: Vec<String> = usage_forms()
            .iter()
            .filter(|u| u.cmd == *cmd && u.opts.contains(f) && !forms.iter().any(|x| std::ptr::eq(*x, *u)))
            .map(|u| if u.keys.is_empty() && !u.exts.is_empty() { format!("{cmd} .{}", u.exts.join("/.")) } else if u.keys.is_empty() { cmd.clone() } else { format!("{cmd} {}", u.keys.join("|")) })
            .collect();
        if !owners.is_empty() {
            hint = format!("{hint} ({f} applies to: {})", owners.join(", "));
        }
        return Err(format!("unknown option {f} for '{label}'{hint}; see: armatura {cmd} --help"));
    }
    Ok(())
}

/// A bake that read events but produced nothing (typo'd ids, wrong source, every key skipped) is a failure, not a success.
fn check_bake_not_empty(a: &Args, inputs: usize, r: &armatura_core::liveinput::BakeReport, cmd: &str, what: &str) -> Result<(), String> {
    if inputs > 0 && r.channels == 0 && r.keys_set == 0 && r.takes == 0 && r.events == 0 && !a.flags.iter().any(|f| f == "--allow-empty") {
        return Err(format!("{cmd}: {inputs} input(s) were read but nothing was produced (0 channels, 0 keys, 0 takes, 0 events); check the ids in {what}; nothing was written (use --allow-empty to accept an empty result)"));
    }
    Ok(())
}

fn num(a: &Args, key: &str, default: f32) -> Result<f32, String> {
    match a.opts.get(key) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| format!("{key} needs a number, got '{v}'")),
    }
}

fn onion_spec_from(a: &Args) -> Result<overlays::OnionSpec, String> {
    let d = overlays::OnionSpec { enabled: true, ..Default::default() };
    let whole = |k: &str, def: u32| -> Result<u32, String> {
        let v = num(a, k, def as f32)?;
        if v < 0.0 || v.fract() != 0.0 {
            return Err(format!("{k} needs a whole number"));
        }
        Ok(v as u32)
    };
    let spec = overlays::OnionSpec {
        before: whole("--before", d.before)?,
        after: whole("--after", d.after)?,
        step: whole("--step", d.step)?,
        mode: if a.flags.iter().any(|f| f == "--keyed") { overlays::OnionMode::Keyed } else { overlays::OnionMode::Relative },
        tint_before: a.opts.get("--tint-before").cloned().unwrap_or(d.tint_before.clone()),
        tint_after: a.opts.get("--tint-after").cloned().unwrap_or(d.tint_after.clone()),
        opacity: num(a, "--opacity", d.opacity)?,
        falloff: num(a, "--falloff", d.falloff)?,
        outline: a.flags.iter().any(|f| f == "--outline"),
        moving_only: !a.flags.iter().any(|f| f == "--all-parts"),
        layers: a.opts.get("--layers").map(|l| l.split(',').filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default(),
        ..d
    };
    spec.validate()?;
    Ok(spec)
}

fn write_frame(e: &Engine, frame: f32, out: &Path, a: &Args) -> Result<(), String> {
    let bones = a.flags.iter().any(|f| f == "--bones");
    let png = out.extension().map(|x| x.eq_ignore_ascii_case("png")).unwrap_or(false);
    let svg = out.extension().map(|x| x.eq_ignore_ascii_case("svg")).unwrap_or(false);
    if !png && !svg {
        return Err(format!("render writes .png or .svg; '{}' has another extension (nothing was written)", out.display()));
    }
    let proxy = match a.opts.get("--proxy") {
        None => 0u8,
        Some(v) => v.parse::<u8>().map_err(|_| format!("--proxy needs a level 0..={}, got '{v}'", armatura_core::scrub::MAX_LEVEL))?,
    };
    armatura_core::scrub::check_level(proxy)?;
    e.evaluate_checked(frame)?;
    let bytes = if proxy > 0 && png {
        armatura_core::scrub::render_proxy_png(e, frame, proxy, a.scale, bones)?
    } else if proxy > 0 {
        armatura_core::scrub::proxy_svg_vector(e, frame, proxy, bones)?.into_bytes()
    } else if png {
        render_png(e, frame, &RasterOptions { scale: a.scale, show_bones: bones, transparent: false })?
    } else {
        render_frame(e, frame, &RenderOptions { show_bones: bones }).into_bytes()
    };
    fwrite(out, bytes)
}

/// `render-gpu`: wgpu render of one frame to PNG; `--bench N` also times N GPU and CPU renders.
#[cfg(feature = "gpu")]
fn render_gpu_cmd(a: &Args, path: &str, out: &str) -> Result<(), String> {
    use armatura_gpu::{AdapterChoice, GpuContext, GpuOptions};
    let e = load_sized(path, a)?;
    let frame: f32 = match a.opts.get("--frame") {
        None => 0.0,
        Some(v) => v.parse().map_err(|_| format!("bad frame number '{v}'"))?,
    };
    let choice: AdapterChoice = a.opts.get("--adapter").map(|s| s.parse()).transpose()?.unwrap_or(AdapterChoice::Auto);
    let ctx = GpuContext::new(choice).map_err(|e| e.to_string())?;
    let opts = GpuOptions { scale: a.scale, show_bones: a.flags.iter().any(|f| f == "--bones"), ..Default::default() };
    let img = armatura_gpu::render(&ctx, &e, frame, &opts).map_err(|e| e.to_string())?;
    fwrite(out, img.encode_png()?)?;
    println!("wrote {out} ({}x{}) on {}", img.width, img.height, ctx.adapter);
    if ctx.adapter.is_software {
        println!("note: software adapter (CPU implementation of the GPU API); timings below say nothing about real GPUs");
    }
    if let Some(n) = a.opts.get("--bench") {
        let n: usize = n.parse().ok().filter(|n| *n > 0 && *n <= 10_000).ok_or("--bench needs a count in 1..=10000")?;
        let f = e.evaluate(frame);
        let t = std::time::Instant::now();
        for _ in 0..n {
            armatura_gpu::render_frame(&ctx, &e, &f, &opts).map_err(|e| e.to_string())?;
        }
        let gpu_ms = t.elapsed().as_secs_f64() * 1e3 / n as f64;
        let ro = RasterOptions { scale: a.scale, show_bones: false, transparent: false };
        let t = std::time::Instant::now();
        for _ in 0..n {
            armatura_core::raster::pixmap_from_frame(&e, &f, frame, &ro);
        }
        let cpu_ms = t.elapsed().as_secs_f64() * 1e3 / n as f64;
        println!("bench ({n} runs, evaluate excluded): gpu {gpu_ms:.2} ms/frame, cpu (tiny-skia) {cpu_ms:.2} ms/frame, supersample {}x, {} passes", img.timing.supersample, img.timing.passes);
    }
    Ok(())
}

/// Writes a project file. The text is parsed back and run through `Engine::new` first, so a generator can never
/// leave a file behind that the other commands then reject; the real cause is reported ("output would be
/// invalid: ..."). The write is atomic (temp file + rename; symbolic links are refused).
///
/// Relative asset paths (image files, normal maps, sound tracks) are validated against the directory of the INPUT
/// project. When the output goes to another directory they are rewritten to stay valid from there if the file lies
/// below the output directory; otherwise a warning on stderr says which assets the output will not find (a project
/// may not use `..`).
fn write_json(p: &Project, out: &str) -> Result<(), String> {
    let text = p.to_json_pretty();
    validate_project_text(&text)?;
    let from = armatura_core::assets::default_dir();
    let to = asset_dir(out);
    let mut q = p.clone();
    let notes = armatura_core::assets::rewrite_relative_assets(&mut q, &from, &to);
    for n in &notes {
        eprintln!("warning: {out}: {n}");
    }
    let text = if q == *p { text } else { q.to_json_pretty() };
    fwrite(out, text.as_bytes())
}

/// `output would be invalid: <cause>` unless `text` is a project that parses and passes `Engine::new`.
fn validate_project_text(text: &str) -> Result<(), String> {
    let back = Project::from_json(text).map_err(|e| format!("output would be invalid: {e}"))?;
    Engine::new(back).map(|_| ()).map_err(|errs| format!("output would be invalid: {}", errs.join("; ")))
}

/// Atomic file write used for every output of the CLI (see `armatura_core::fsutil::write_atomic`).
fn fwrite(out: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<(), String> {
    let out = out.as_ref();
    let special = fs::metadata(out).map(|m| !m.is_file()).unwrap_or(false);
    if special {
        // /dev/stdout, a FIFO, ...: nothing to rename over
        return fs::write(out, data).map_err(|e| format!("cannot write {}: {e}", out.display()));
    }
    armatura_core::fsutil::write_atomic(out, data.as_ref()).map_err(|e| format!("cannot write {}: {e}", out.display()))
}

/// Read a motion library (`take-record` file): every error names the file.
fn load_library(path: &str) -> Result<motionlib::Library, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read the take library {path}: {e}"))?;
    motionlib::Library::from_json(&text).map_err(|e| format!("{path}: {e}"))
}

/// `from=to,from=to` retargeting words of `take-apply`: every entry must be `from=to` and `from` must be a target the
/// take really has (a stray word such as `speed=1` used to be taken for a rename of a target called `speed`).
fn parse_take_rename(s: &str, take: &motionlib::Take) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for item in s.split(',').filter(|x| !x.is_empty()) {
        let Some((from, to)) = item.split_once('=').filter(|(f, t)| !f.is_empty() && !t.is_empty()) else {
            return Err(format!("'{item}' is not a rename: write from=to (take target = project target), or '-' for none; the speed is a plain number after it (nothing was written)"));
        };
        if !take.tracks.iter().any(|t| t.target == from) {
            let mut targets: Vec<&str> = take.tracks.iter().map(|t| t.target.as_str()).collect();
            targets.sort();
            targets.dedup();
            return Err(format!("rename '{item}': take '{}' has no track for '{from}' (its targets: {}); the speed is a plain number, not speed=N (nothing was written)", take.name, targets.join(", ")));
        }
        out.push((from.to_string(), to.to_string()));
    }
    Ok(out)
}

/// A frame given on the command line must lie in `0..=frames-1` (the studio's `/api/frame` refuses anything else).
/// `clamp` opts into the nearest valid frame instead of the error.
fn check_frame_in_range(frame: f32, frames: u32, clamp: bool) -> Result<f32, String> {
    let last = frames.saturating_sub(1) as f32;
    if !frame.is_finite() {
        return Err(format!("frame {frame} is not a number the animation has (valid frames are 0..{last})"));
    }
    if (0.0..=last).contains(&frame) {
        return Ok(frame);
    }
    if clamp {
        let c = frame.clamp(0.0, last);
        eprintln!("warning: frame {frame} is outside the animation (0..{last}); --clamp renders frame {c}");
        return Ok(c);
    }
    Err(format!("frame {frame} is outside the animation: the project has {frames} frames, so the valid frames are 0..{last}; pass --clamp to render the nearest frame instead"))
}

/// Refuses an output path that is the input project itself (render/export/... would silently destroy it).
fn guard_out(input: &str, out: &str) -> Result<(), String> {
    let same = match (fs::canonicalize(input), fs::canonicalize(out)) {
        (Ok(a), Ok(b)) => a == b,
        _ => Path::new(input) == Path::new(out),
    };
    if same {
        Err(format!("refusing to overwrite the input project {input} with output of the same name"))
    } else {
        Ok(())
    }
}

/// Extra ffmpeg arguments that mix the project's sound tracks (none when it has no sounds).
/// Inputs must come before the output; the filter/map/codec go after the video options.
fn audio_inputs(e: &Engine, base: &Path) -> Result<(Vec<String>, Vec<String>, bool), String> {
    Ok(match soundtrack::mux_args(&e.project, base, 1, None)? {
        Some(m) => (m.inputs, vec!["-filter_complex".into(), m.filter, "-map".into(), "0:v".into(), "-map".into(), "[aout]".into(), "-t".into(), format!("{}", e.project.frames as f32 / e.project.fps)], true),
        None => (vec![], vec![], false),
    })
}

/// Seconds from `1.5`, `1.5s` or a frame count such as `36f` (at the project frame rate).
fn parse_time(s: &str, fps: f32, what: &str) -> Result<f32, String> {
    let (num, scale) = match s.strip_suffix('f') {
        Some(n) => (n, 1.0 / fps),
        None => (s.strip_suffix('s').unwrap_or(s), 1.0),
    };
    num.parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| v * scale).ok_or_else(|| format!("{what} needs seconds (1.5) or frames (36f), got '{s}'"))
}

/// Apply --trim-in/--trim-out/--fade-in/--fade-out to a track (a trim-out of 0 means "to the end").
fn apply_sound_opts(t: &mut soundtrack::SoundTrack, a: &Args, fps: f32) -> Result<(), String> {
    if let Some(v) = a.opts.get("--trim-in") { t.trim_in = parse_time(v, fps, "--trim-in")?; }
    if let Some(v) = a.opts.get("--trim-out") { let o = parse_time(v, fps, "--trim-out")?; t.trim_out = if o == 0.0 { None } else { Some(o) }; }
    if let Some(v) = a.opts.get("--fade-in") { t.fade_in = parse_time(v, fps, "--fade-in")?; }
    if let Some(v) = a.opts.get("--fade-out") { t.fade_out = parse_time(v, fps, "--fade-out")?; }
    Ok(())
}

/// GIF and APNG have no audio track: say so instead of silently dropping the sounds.
/// Prints every export warning on stderr and, with `--report FILE`, writes them as JSON
/// (`{"format","count","warnings":[{"code","layer","message"}]}`). The file may not be the input project.
fn report_warnings(a: &Args, project: &str, format: &str, warns: Vec<(&'static str, String, String)>) -> Result<(), String> {
    for (code, layer, msg) in &warns {
        if layer.is_empty() {
            eprintln!("warning: [{code}] {msg}");
        } else {
            eprintln!("warning: [{code}] {layer}: {msg}");
        }
    }
    if let Some(r) = a.opts.get("--report") {
        guard_out(project, r)?;
        let v = serde_json::json!({
            "format": format,
            "count": warns.len(),
            "warnings": warns.iter().map(|(c, l, m)| serde_json::json!({"code": c, "layer": l, "message": m})).collect::<Vec<_>>(),
        });
        fwrite(r, serde_json::to_string_pretty(&v).unwrap()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn warn_silent(e: &Engine, kind: &str) {
    if !e.project.sounds.is_empty() {
        eprintln!("note: {kind} cannot carry audio; the project's {} sound track(s) are not included (use .mp4, .webm or .html)", e.project.sounds.len());
    }
}

/// The mixed sound as an mp3 for the HTML player, or None (with a warning on stderr) when there is
/// none, ffmpeg fails, or it would be too large to embed.
fn html_audio(e: &Engine, base: &Path) -> Option<(&'static str, Vec<u8>)> {
    if e.project.sounds.is_empty() {
        return None;
    }
    match soundio::mix_bytes(&e.project, base, "mp3") {
        Ok((_, b)) if b.len() > export::HTML_AUDIO_MAX => {
            eprintln!("warning: the mixed audio is {:.1} MB (limit {} MB for the HTML player); exporting without sound - shorten the sounds or use .mp4", b.len() as f64 / 1048576.0, export::HTML_AUDIO_MAX >> 20);
            None
        }
        Ok((m, b)) => {
            if b.len() > export::HTML_AUDIO_WARN {
                eprintln!("warning: the embedded audio is {:.1} MB; the HTML file will be about {:.1} MB larger", b.len() as f64 / 1048576.0, b.len() as f64 * 1.34 / 1048576.0);
            }
            Some((m, b))
        }
        Err(err) => {
            eprintln!("warning: audio not embedded: {err}");
            None
        }
    }
}

fn export_mp4(e: &Engine, out: &str, scale: f32, base: &Path) -> Result<(), String> {
    armatura_core::fsutil::refuse_symlink(Path::new(out))?;
    let guard = armatura_core::fsutil::private_temp_dir("armatura-mp4")?;
    let dir = guard.path().to_path_buf();
    let (ain, aargs, has_audio) = audio_inputs(e, base)?;
    let result = (|| {
        for f in 0..e.project.frames {
            let png = render_png(e, f as f32, &RasterOptions { scale, show_bones: false, transparent: false })?;
            fwrite(dir.join(format!("f_{f:05}.png")), png).map_err(|e| e.to_string())?;
        }
        let status = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-protocol_whitelist", "file", "-framerate", &e.project.fps.to_string(), "-i"])
            .arg(dir.join("f_%05d.png"))
            .args(&ain)
            .args(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-pix_fmt", "yuv420p", "-c:v", "libx264"])
            .args(&aargs)
            .args(if has_audio { &["-c:a", "aac"][..] } else { &[][..] })
            .args(["-protocol_whitelist", "file"])
            .arg(armatura_core::fsutil::file_arg(out))
            .status()
            .map_err(|e| format!("cannot run ffmpeg ({e}); install it or export a GIF/HTML instead"))?;
        if status.success() { Ok(()) } else { Err("ffmpeg failed".to_string()) }
    })();
    drop(guard);
    result
}

/// WebM with alpha. VP9 (default) or AV1, encoded by the user's own ffmpeg (external process, never bundled); patent/licensing status of the codecs is not assessed here (docs/PATENT_NOTES.md). Needs ffmpeg on PATH.
fn export_webm(e: &Engine, out: &str, scale: f32, codec: &str, base: &Path) -> Result<(), String> {
    let enc: &[&str] = match codec {
        "vp9" => &["-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p", "-b:v", "0", "-crf", "30", "-auto-alt-ref", "0"],
        "av1" => &["-c:v", "libaom-av1", "-pix_fmt", "yuva420p", "-b:v", "0", "-crf", "35", "-cpu-used", "6"],
        other => return Err(format!("unknown --codec '{other}' (vp9 or av1)")),
    };
    armatura_core::fsutil::refuse_symlink(Path::new(out))?;
    let guard = armatura_core::fsutil::private_temp_dir("armatura-webm")?;
    let dir = guard.path().to_path_buf();
    let (ain, aargs, has_audio) = audio_inputs(e, base)?;
    let result = (|| {
        for f in 0..e.project.frames {
            let png = render_png(e, f as f32, &RasterOptions { scale, show_bones: false, transparent: false })?;
            fwrite(dir.join(format!("f_{f:05}.png")), png).map_err(|e| e.to_string())?;
        }
        let status = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-protocol_whitelist", "file", "-framerate", &e.project.fps.to_string(), "-i"])
            .arg(dir.join("f_%05d.png"))
            .args(&ain)
            .args(enc)
            .args(&aargs)
            .args(if has_audio { &["-c:a", "libopus"][..] } else { &[][..] })
            .args(["-protocol_whitelist", "file"])
            .arg(armatura_core::fsutil::file_arg(out))
            .status()
            .map_err(|e| format!("cannot run ffmpeg ({e}); install it or export an APNG instead"))?;
        if status.success() { Ok(()) } else { Err("ffmpeg failed (does your build include the chosen codec?)".to_string()) }
    })();
    drop(guard);
    result
}

/// Encode already rendered PNG frames as .mp4 (H.264) or .webm (VP9/AV1 with alpha) through the user's ffmpeg.
/// Before any frame is rendered: the output's directory exists and, for video, ffmpeg is there and lists the encoder
/// the chosen format needs (a failure after minutes of rendering used to be the first sign).
fn dof_export_preflight(out: &str, a: &Args) -> Result<(), String> {
    let lower = out.to_lowercase();
    let is_video = lower.ends_with(".mp4") || lower.ends_with(".webm");
    let is_file = is_video || lower.ends_with(".gif") || lower.ends_with(".apng");
    if is_file {
        let dir = asset_dir(out);
        if !dir.is_dir() {
            return Err(format!("dof-export: the directory {} does not exist (nothing was rendered; create it first)", dir.display()));
        }
    }
    if is_video {
        let webm = lower.ends_with(".webm");
        let codec = a.opts.get("--codec").map(String::as_str).unwrap_or("vp9");
        let encoder = if webm {
            match codec {
                "vp9" => "libvpx-vp9",
                "av1" => "libaom-av1",
                other => return Err(format!("unknown --codec '{other}' (vp9 or av1)")),
            }
        } else {
            "libx264"
        };
        let listing = Command::new("ffmpeg").args(["-hide_banner", "-loglevel", "error", "-encoders"]).output().map_err(|e| format!("dof-export: cannot run ffmpeg ({e}); install it or export a GIF/APNG instead (nothing was rendered)"))?;
        if !String::from_utf8_lossy(&listing.stdout).split_whitespace().any(|w| w == encoder) {
            return Err(format!("dof-export: this ffmpeg has no {encoder} encoder, needed for {} (nothing was rendered); use another format or codec", if webm { "WebM" } else { "MP4" }));
        }
    }
    Ok(())
}

fn encode_png_video(pngs: &[Vec<u8>], fps: f32, out: &str, webm: bool, codec: &str) -> Result<(), String> {
    let enc: Vec<&str> = if webm {
        match codec {
            "vp9" => vec!["-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p", "-b:v", "0", "-crf", "30", "-auto-alt-ref", "0"],
            "av1" => vec!["-c:v", "libaom-av1", "-pix_fmt", "yuva420p", "-b:v", "0", "-crf", "35", "-cpu-used", "6"],
            other => return Err(format!("unknown --codec '{other}' (vp9 or av1)")),
        }
    } else {
        vec!["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-pix_fmt", "yuv420p", "-c:v", "libx264"]
    };
    armatura_core::fsutil::refuse_symlink(Path::new(out))?;
    let guard = armatura_core::fsutil::private_temp_dir("armatura-dofvid")?;
    let dir = guard.path().to_path_buf();
    let result = (|| {
        for (i, b) in pngs.iter().enumerate() {
            fwrite(dir.join(format!("f_{i:05}.png")), b).map_err(|e| e.to_string())?;
        }
        let status = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-protocol_whitelist", "file", "-framerate", &fps.to_string(), "-i"])
            .arg(dir.join("f_%05d.png"))
            .args(&enc)
            .args(["-protocol_whitelist", "file"])
            .arg(armatura_core::fsutil::file_arg(out))
            .status()
            .map_err(|e| format!("cannot run ffmpeg ({e}); install it or export a GIF/APNG instead"))?;
        if status.success() { Ok(()) } else { Err("ffmpeg failed (does your build include the chosen codec?)".to_string()) }
    })();
    drop(guard);
    result
}

/// Read a user-authored config file; unknown keys (typos) are an error, not silently ignored.
fn read_json<T: serde::Serialize + serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let text = read_text_capped(path)?;
    armatura_core::unknownfields::parse_strict(&text).map_err(|e| format!("invalid JSON in {path}: {e}"))
}

fn pose_suggester(a: &Args, project: &Project, db_path: &str) -> Result<posesuggest::Suggester, String> {
    let db = if db_path == "-" {
        let lib = match a.opts.get("--library") {
            Some(p) => Some(read_json::<armatura_core::motionlib::Library>(p)?),
            None => None,
        };
        posesuggest::build_db(Some(project), lib.as_ref(), None)?
    } else {
        posesuggest::PoseDb::from_json(&fs::read_to_string(db_path).map_err(|e| format!("cannot read {db_path}: {e}"))?)?
    };
    posesuggest::Suggester::new(db)
}

fn print_suggestions(s: &posesuggest::Suggestions) {
    println!("{}: {} library poses, {} PCA axes ({:.0}% of the variance)", s.kind, s.poses_in_library, s.pca_axes, s.pca_explained * 100.0);
    for n in &s.notes {
        println!("  note: {n}");
    }
    for (i, c) in s.candidates.iter().enumerate() {
        let err = c.constraint_error.map(|e| format!("  constraint error {e:.2}")).unwrap_or_default();
        println!("{:>2}. {:<11} score {:.2}  plausibility {:.2}{err}  key at frame {:.1}", i + 1, c.method, c.score, c.plausibility, c.frame.unwrap_or(0.0));
        println!("      {}", c.label);
        println!("      {}", c.values.iter().map(|v| format!("{}={:.1}", v.bone, v.angle)).collect::<Vec<_>>().join(" "));
        for n in &c.notes {
            println!("      note: {n}");
        }
    }
    println!("(scores rank candidates against this library; they are heuristics, not probabilities)");
}

fn run(raw: &[String]) -> Result<(), String> {
    let expanded = expand_eq(raw)?;
    let raw = expanded.as_slice();
    if let Some(n) = raw.iter().position(|x| x == "--max-input-bytes") {
        let v = raw.get(n + 1).ok_or("--max-input-bytes needs a value")?;
        let n: u64 = v.parse().ok().filter(|n| *n > 0).ok_or_else(|| format!("--max-input-bytes needs a positive whole number, got '{v}'"))?;
        MAX_INPUT_BYTES.store(n, std::sync::atomic::Ordering::SeqCst);
    }
    let a = parse_args(raw)?;
    check_flags(&a, raw)?;
    let pos: Vec<&str> = a.pos.iter().map(String::as_str).collect();
    if let Some(c) = pos.first().filter(|_| a.flags.iter().any(|f| f == "--help")) {
        if let Some(u) = usage_for(c) {
            println!("{u}");
            return Ok(());
        }
    }
    if !pos.is_empty() && a.flags.iter().any(|f| f == "--version") {
        println!("armatura {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if pos.is_empty() && a.flags.iter().any(|f| f == "--version") {
        println!("armatura {} (armatura-cli, AGPL-3.0-or-later; engine libraries MPL-2.0)\n{}", env!("CARGO_PKG_VERSION"), studio::source_offer_text(studio::default_source_url()));
        return Ok(());
    }
    // commands that read a project and write a different kind of file: never over the project itself
    match pos.as_slice() {
        ["render", p, _, o] | ["render-hdr", p, _, o] | ["render-hdr", p, _, o, _] | ["dof", p, _, o] | ["dof-export", p, o]
        | ["export", p, o] | ["layout-preview", p, o] | ["render-gpu", p, o] | ["export-fit", p, o] | ["fuzz-grid", p, o]
        | ["onion", p, o] | ["onion", p, _, o] | ["trail", p, _, o] | ["golden", _, p, o] => guard_out(p, o)?,
        // the Lottie file is the source of the import: never overwrite it with the project (or the effects file)
        ["import-lottie", inp, o, rest @ ..] => {
            guard_out(inp, o)?;
            if let Some(fx) = rest.first() {
                guard_out(inp, fx)?;
                guard_out(o, fx)?;
            }
        }
        _ => {}
    }
    match pos.as_slice() {
        ["demo", out] => {
            write_json(&demo::project(), out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["demo-fx", out] => {
            write_json(&demo::fx_project(), out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["check", path] => {
            let e = load(path)?;
            for w in unknown_field_warnings(path) {
                println!("warning: {w} (ignored)");
            }
            let p = &e.project;
            for (layer, msg) in lint::gradient_warnings(p).into_iter().chain(lint::switch_key_warnings(p)) {
                println!("warning: {layer}: {msg}");
            }
            // parts rendering never reads (skins, comps, markers, events, shots, channel_meta, bone_vis, clips, lods) and values it clamps
            for w in armatura_core::editordata::warning_lines(p) {
                println!("warning: {w}");
            }
            let mut bad_sounds = Vec::new();
            for s in &p.sounds {
                if let Err(err) = soundtrack::check_file(&asset_dir(path), &s.file) {
                    bad_sounds.push(format!("sound '{}': {err}", s.name));
                }
            }
            if !bad_sounds.is_empty() {
                return Err(format!("{path} has {} unusable sound track(s):\n  - {}", bad_sounds.len(), bad_sounds.join("\n  - ")));
            }
            println!(
                "ok: '{}' {}×{} @ {} fps, {} frames, {} bones, {} ik, {} layers, {} channels",
                p.name, p.width, p.height, p.fps, p.frames, p.bones.len(), p.ik.len(), e.node_count(), p.channels.len()
            );
            Ok(())
        }
        ["render", path, frame, out] => {
            let e = load_sized(path, &a)?;
            let frame: f32 = frame.parse().map_err(|_| format!("bad frame number '{frame}'"))?;
            let frame = check_frame_in_range(frame, e.project.frames, a.flags.iter().any(|f| f == "--clamp"))?;
            write_frame(&e, frame, Path::new(out), &a)?;
            println!("wrote {out}");
            Ok(())
        }
        ["frames", path, dir] => {
            let e = load_sized(path, &a)?;
            e.check_all_frames_finite()?;
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            let ext = if a.flags.iter().any(|f| f == "--png") { "png" } else { "svg" };
            for f in 0..e.project.frames {
                write_frame(&e, f as f32, &Path::new(dir).join(format!("frame_{f:04}.{ext}")), &a)?;
            }
            println!("wrote {} frames to {dir}", e.project.frames);
            Ok(())
        }
        ["export", path, out] => {
            let e = load_sized(path, &a)?;
            e.check_all_frames_finite()?;
            if matches!(Path::new(out).extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("json" | "lottie" | "html" | "htm" | "glb")) {
                export::check_budget(&e.project)?;
            }
            let ext = Path::new(out).extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext == "png" {
                guard_out(path, &Path::new(out).with_extension("atlas.json").to_string_lossy())?;
            }
            match ext.as_str() {
                "gif" => {
                    warn_silent(&e, "GIF");
                    let (bytes, warns) = export::gif_bytes_warn(&e, a.scale)?;
                    report_warnings(&a, path, "gif", warns.iter().map(|w| ("frame-rate", String::new(), w.clone())).collect())?;
                    fwrite(out, bytes).map_err(|e| e.to_string())?
                }
                "html" | "htm" => {
                    let au = html_audio(&e, Path::new(path).parent().unwrap_or(Path::new(".")));
                    fwrite(out, export::html_player_with_audio(&e, au.as_ref().map(|(m, b)| (*m, b.as_slice())))).map_err(|e| e.to_string())?
                }
                "json" => {
                    let x = export::lottie_export(&e);
                    report_warnings(&a, path, "lottie", x.warnings.iter().map(|w| (w.code, w.layer.clone(), w.message.clone())).collect())?;
                    fwrite(out, x.doc.to_string()).map_err(|e| e.to_string())?
                }
                "mp4" => export_mp4(&e, out, a.scale, Path::new(path).parent().unwrap_or(Path::new(".")))?,
                "lottie" => {
                    let (bytes, warns) = open_formats::dotlottie_bytes_report(&e)?;
                    report_warnings(&a, path, "dotlottie", warns.iter().map(|w| (w.code, w.layer.clone(), w.message.clone())).collect())?;
                    fwrite(out, bytes).map_err(|e| e.to_string())?
                }
                "apng" => { warn_silent(&e, "APNG"); fwrite(out, open_formats::apng_bytes(&e, a.scale)?).map_err(|e| e.to_string())? }
                "glb" => {
                    let (bytes, notes) = armatura_core::gltf::export_glb_report(&e)?;
                    fwrite(out, bytes).map_err(|e| e.to_string())?;
                    report_warnings(&a, path, "glb", notes.into_iter().map(|n| ("glb-limit", String::new(), n)).collect())?;
                }
                "webm" => export_webm(&e, out, a.scale, a.opts.get("--codec").map(String::as_str).unwrap_or("vp9"), Path::new(path).parent().unwrap_or(Path::new(".")))?,
                "png" => {
                    let cols = num(&a, "--columns", 0.0)? as usize;
                    // `<name>.atlas.json`, never `<name>.json`: that name is where a Lottie export of the same stem goes
                    let json_path = Path::new(out).with_extension("atlas.json");
                    let name = Path::new(out).file_name().and_then(|n| n.to_str()).unwrap_or("sheet.png");
                    let (img, meta) = open_formats::spritesheet(&e, a.scale, cols, name)?;
                    fwrite(out, img).map_err(|e| e.to_string())?;
                    fwrite(&json_path, serde_json::to_string_pretty(&meta).unwrap()).map_err(|e| e.to_string())?;
                    println!("wrote {}", json_path.display());
                }
                "" => return Err(format!("'{out}' has no file extension; the extension picks the format (use .gif .html .htm .json .lottie .mp4 .apng .webm .png .glb)")),
                other => return Err(format!("unknown export format '.{other}' (use .gif .html .htm .json .lottie .mp4 .apng .webm .png .glb)")),
            }
            println!("wrote {out}");
            Ok(())
        }
        ["lipsync", path, wav, layer, out] => {
            let mut p = load_project(path)?;
            let audio = read_wav_file(wav)?;
            let mapping: [usize; 5] = match a.opts.get("--map") {
                None => armatura_core::audio::default_lipsync_map(armatura_core::audio::switch_children(&p, layer).unwrap_or(5)),
                Some(m) => {
                    let v: Vec<usize> = m.split(',').map(|s| s.trim().parse().map_err(|_| format!("bad --map entry '{s}'"))).collect::<Result<_, _>>()?;
                    v.try_into().map_err(|_| "--map needs exactly 5 numbers (rest,small,open,round,wide)".to_string())?
                }
            };
            let ch = armatura_core::audio::lipsync_checked(&p, layer, &audio, &mapping)?;
            p.channels.retain(|c| !(c.target == *layer && c.prop == Prop::LayerSwitch && matches!(c.source, Source::Keys { .. })));
            p.channels.push(ch);
            compile(p.clone(), path)?; // validates the layer exists and is a switch
            write_json(&p, out)?;
            println!("wrote {out} ({:.2}s of audio → {} frames)", audio.duration(), p.frames);
            Ok(())
        }
        ["walk", path, out] => {
            let mut p = load_project(path)?;
            let g = |k: &str| a.opts.get(k).cloned();
            let rig = WalkRig {
                hips: g("--hips"),
                thigh_l: g("--thigh-l"),
                thigh_r: g("--thigh-r"),
                shin_l: g("--shin-l"),
                shin_r: g("--shin-r"),
                arm_l: g("--arm-l"),
                arm_r: g("--arm-r"),
            };
            let d = WalkStyle::default();
            let style = WalkStyle {
                period: num(&a, "--period", d.period)?,
                stride: num(&a, "--stride", d.stride)?,
                knee: num(&a, "--knee", d.knee)?,
                bob: num(&a, "--bob", d.bob)?,
                arm_swing: num(&a, "--swing", d.arm_swing)?,
            };
            let ch = walk_cycle(&rig, &style);
            if ch.is_empty() {
                return Err("name at least one bone (--thigh-l, --thigh-r, …)".into());
            }
            let n = ch.len();
            p.channels.extend(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (+{n} channels)");
            Ok(())
        }
        ["mirror", path, out] => {
            let mut p = load_project(path)?;
            let before = p.channels.clone();
            p.channels = mirror(&p.channels);
            let swapped = before.iter().zip(&p.channels).filter(|(a, b)| a.target != b.target).count();
            if before.is_empty() {
                eprintln!("warning: mirror: the project has no channels, so there is nothing to mirror; {out} is a copy of the input");
            } else if swapped == 0 {
                eprintln!("warning: mirror: no channel target has a left/right name (L_/R_ prefix, _L/_R or _l/_r suffix, left/right, L./R.), so no target was swapped; only the sign of rotations, horizontal moves and skew was flipped on {} channel(s)", p.channels.iter().zip(&before).filter(|(a, b)| a != b).count());
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} ({swapped} of {} channel target(s) swapped sides)", before.len());
            Ok(())
        }
        ["export-oca", path, dir] => {
            let p = load_project(path)?;
            compile(p.clone(), path)?.check_all_frames_finite()?;
            let f = open_formats::write_oca(&p, &asset_dir(path), Path::new(dir), a.scale, a.flags.iter().any(|f| f == "--layers"))?;
            println!("wrote {}", f.display());
            Ok(())
        }
        ["import-ora", file, out] => {
            let bytes = fs::read(file).map_err(|e| format!("cannot read {file}: {e}"))?;
            let name = Path::new(file).file_stem().and_then(|n| n.to_str()).unwrap_or("ora");
            let p = open_formats::import_ora(&bytes, name, num(&a, "--fps", 24.0)?, num(&a, "--frames", 1.0)? as u32)?;
            Engine::new(p.clone()).map_err(|errs| errs.join("; "))?;
            write_json(&p, out)?;
            println!("wrote {out} ({} layers)", p.layers.len());
            Ok(())
        }
        ["safety", path] => {
            let e = load(path)?;
            let r = safety::analyze_engine(&e, a.scale.min(1.0));
            let m = safety::motion_report(&e);
            let line = |n: &str, f: &safety::FlashFinding| {
                let v = if f.fails { "FAIL" } else if f.warns { "WARN" } else { "ok" };
                println!("{n:<12} {v:<5} worst {} transitions/s (frame {})", f.worst_transitions, f.worst_frame);
            };
            line("flashes", &r.general);
            line("red flashes", &r.red);
            println!("camera       pan {:.2} widths/s, rotation {:.0}°/s, zoom ×{:.2}/s", m.max_pan_per_sec, m.max_rotation_deg_per_sec, m.max_zoom_ratio_per_sec);
            for w in &m.warnings {
                println!("  warning: {w}");
            }
            println!("(engineering aid based on the public WCAG 2.3.1 definition; not a broadcast certification)");
            if r.passes() { Ok(()) } else { Err("flash limits exceeded".into()) }
        }
        ["reduced-motion", path, out] => {
            let p = load_project(path)?;
            let q = safety::reduced_motion_twin(&p, num(&a, "--factor", 0.25)?);
            compile(q.clone(), path)?;
            write_json(&q, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["pose-db", "build", out] => {
            let project = match a.opts.get("--project") {
                Some(p) => Some(load_project(p)?),
                None => None,
            };
            let lib = match a.opts.get("--library") {
                Some(p) => Some(read_json::<armatura_core::motionlib::Library>(p)?),
                None => None,
            };
            if project.is_none() && lib.is_none() {
                return Err("give --project and/or --library".into());
            }
            let step = if a.opts.contains_key("--step") { Some(num(&a, "--step", 1.0)?) } else { None };
            let db = posesuggest::build_db(project.as_ref(), lib.as_ref(), step)?;
            fwrite(out, db.to_json())?;
            println!("wrote {out}: {} poses of {} bones in {} sequence(s)", db.poses.len(), db.dim(), db.sequences.len());
            Ok(())
        }
        ["pose-db", "info", path] => {
            let db = posesuggest::PoseDb::from_json(&fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?)?;
            let sg = posesuggest::Suggester::new(db)?;
            println!("{} poses, {} bones: {}", sg.db.poses.len(), sg.db.dim(), sg.db.bones.join(", "));
            for (i, s) in sg.db.sequences.iter().enumerate() {
                println!("  sequence {i}: {} ({}, {} poses)", s.name, s.source, s.poses);
            }
            println!("PCA: {} axes keep {:.1}% of the variance (total {:.1} deg^2)", sg.space.k(), sg.space.explained.last().copied().unwrap_or(0.0) * 100.0, sg.space.total_var);
            for (i, (e, c)) in sg.space.eig.iter().zip(&sg.space.explained).enumerate() {
                println!("  axis {}: std {:.2} deg, cumulative {:.1}%", i + 1, e.sqrt(), c * 100.0);
            }
            println!("typical nearest-pose spacing {:.2} deg, typical step between consecutive poses {:.2} deg", sg.spacing, sg.step);
            Ok(())
        }
        ["pose-db", "train-mlp", path, out] => {
            let db = posesuggest::PoseDb::from_json(&fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?)?;
            let mut o = tinymlp::TrainOpts::default();
            if let Some(h) = a.opts.get("--hidden") {
                o.hidden = h.split(',').map(|x| x.trim().parse::<usize>().map_err(|_| format!("--hidden needs comma separated sizes, got '{h}'"))).collect::<Result<_, _>>()?;
            }
            let ep = num(&a, "--epochs", o.epochs as f32)?;
            if !(1.0..=100_000.0).contains(&ep) {
                return Err("--epochs must be between 1 and 100000".into());
            }
            o.epochs = ep as usize;
            o.lr = num(&a, "--lr", o.lr)?;
            o.seed = num(&a, "--seed", 1.0)? as u64;
            let (m, hist) = posesuggest::train_completion(&db, &o)?;
            let bytes = if out.ends_with(".json") { m.to_json().into_bytes() } else { m.to_bytes() };
            fwrite(out, bytes)?;
            println!("wrote {out}: {} -> {:?} -> {} (epochs {}, loss {:.4} -> {:.4})", m.n_in(), o.hidden, m.n_out(), o.epochs, hist[0], hist[hist.len() - 1]);
            println!("trained on the library itself (no held-out data here); run pose-bench for held-out accuracy");
            Ok(())
        }
        ["pose-bench"] => {
            for l in posesuggest::benchmark(1, &posesuggest::default_bench_opts())? {
                println!("{:<11} {:<42} {:>6.2} deg RMS", l.task, l.method, l.rms_deg);
            }
            println!("synthetic gait-like library, 24 training cycles / 6 held-out cycles; completion sees 3 of 10 bones. Lower is better.");
            Ok(())
        }
        ["pose-suggest", kind, path, db] if ["between", "complete", "next"].contains(kind) => {
            let p = load_project(path)?;
            let sg = pose_suggester(&a, &p, db)?;
            let fr = |k: &str| -> Result<Option<f32>, String> { if a.opts.contains_key(k) { Ok(Some(num(&a, k, 0.0)?)) } else { Ok(None) } };
            let mut constraints: Vec<posesuggest::Constraint> = Vec::new();
            if let Some(f) = a.opts.get("--fix") {
                for part in f.split(',').filter(|x| !x.is_empty()) {
                    let (b, v) = part.split_once('=').ok_or_else(|| format!("--fix needs bone=angle, got '{part}'"))?;
                    constraints.push(posesuggest::Constraint::Angle { bone: b.into(), value: v.parse().map_err(|_| format!("bad angle '{v}'"))?, weight: 1.0 });
                }
            }
            if let Some(r) = a.opts.get("--reach") {
                for part in r.split(';').filter(|x| !x.is_empty()) {
                    let (b, xy) = part.split_once('=').ok_or_else(|| format!("--reach needs bone=x,y, got '{part}'"))?;
                    let (x, y) = xy.split_once(',').ok_or_else(|| format!("--reach needs bone=x,y, got '{part}'"))?;
                    constraints.push(posesuggest::Constraint::Reach { bone: b.into(), at: [x.parse().map_err(|_| format!("bad x '{x}'"))?, y.parse().map_err(|_| format!("bad y '{y}'"))?], weight: 0.1 });
                }
            }
            let model = match a.opts.get("--model") {
                Some(m) => Some(tinymlp::Mlp::load(&fs::read(m).map_err(|e| format!("cannot read {m}: {e}"))?)?),
                None => None,
            };
            let req = posesuggest::Request {
                kind: (*kind).into(),
                a: fr("--a")?,
                b: fr("--b")?,
                t: fr("--t")?,
                frame: fr("--frame")?,
                constraints,
                free: a.opts.get("--free").map(|f| f.split(',').filter(|x| !x.is_empty()).map(String::from).collect()),
                top: num(&a, "--top", 5.0)? as usize,
                snap: fr("--snap")?,
                prior: fr("--prior")?,
                k: fr("--k")?.map(|k| k as usize),
            };
            let s = posesuggest::suggest(&p, &sg, &req, model.as_ref())?;
            if a.flags.iter().any(|f| f == "--json") {
                println!("{}", serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?);
            } else {
                print_suggestions(&s);
            }
            if a.opts.contains_key("--apply") {
                let rank = num(&a, "--apply", 1.0)? as usize;
                let out = a.opts.get("--out").ok_or("--apply needs --out <project.json>")?;
                let c = s.candidates.get(rank.wrapping_sub(1)).ok_or_else(|| format!("there is no candidate {rank} (1..{})", s.candidates.len()))?;
                let (op, skipped) = posesuggest::apply_op(&p, &c.values, c.frame.unwrap_or(0.0), armatura_core::anim::Ease::Smooth)?;
                let mut ed = armatura_core::edit::Editor::new(p.clone()).map_err(|e| e.join("; "))?;
                ed.apply(op).map_err(|e| e.join("; "))?;
                write_json(ed.project(), out)?;
                println!("applied candidate {rank} at frame {:.1} -> {out}{}", c.frame.unwrap_or(0.0), if skipped.is_empty() { String::new() } else { format!(" (locked, skipped: {})", skipped.join(", ")) });
            }
            Ok(())
        }
        ["autotrace-bench"] => {
            println!("{:<14} {:<5} {:>6} {:>4} {:>7} {:>7} {:>6} {:>6} {:>7} {:>7} {:>8} {:>7}", "image", "mode", "colors", "tol", "MAE", "RMSE", "PSNR", "SSIM", "regions", "strokes", "anchors", "ms");
            for r in autotrace::benchmark() {
                println!("{:<14} {:<5} {:>6} {:>4} {:>7.2} {:>7.2} {:>6.1} {:>6.3} {:>7} {:>7} {:>8} {:>7.1}", r.name, if r.mode == autotrace::Mode::Fill { "fill" } else { "line" }, r.colors, r.tol, r.fidelity.mae, r.fidelity.rmse, r.fidelity.psnr, r.fidelity.ssim, r.regions, r.strokes, r.anchors, r.millis);
            }
            println!("MAE/RMSE in 0..255 over R,G,B on white; SSIM on luminance, 8x8 windows. Classical tracer: photographs become colour bands.");
            Ok(())
        }
        ["autotrace", image, out] => {
            let bytes = fs::read(image).map_err(|e| format!("cannot read {image}: {e}"))?;
            let img = autotrace::decode_image(&bytes)?;
            let mode = match a.opts.get("--mode").map(String::as_str).unwrap_or("fill") {
                "fill" => autotrace::Mode::Fill,
                "line" => autotrace::Mode::Line,
                m => return Err(format!("--mode must be fill or line, not '{m}'")),
            };
            let mut o = autotrace::TraceOptions { mode, ..autotrace::TraceOptions::default() };
            let whole = |k: &str, d: usize| -> Result<usize, String> {
                let v = num(&a, k, d as f32)?;
                if v < 0.0 || v.fract() != 0.0 {
                    return Err(format!("{k} needs a whole number"));
                }
                Ok(v as usize)
            };
            o.colors = whole("--colors", o.colors)?;
            o.min_area = num(&a, "--min-area", o.min_area)?;
            o.tol = num(&a, "--tol", o.tol)?;
            o.smooth = whole("--smooth", o.smooth)?;
            o.corner_angle = num(&a, "--corner", o.corner_angle)?;
            o.denoise = whole("--denoise", o.denoise)?;
            o.prune = whole("--prune", o.prune)?;
            o.seed = whole("--seed", 1)? as u64;
            o.invert = a.flags.iter().any(|f| f == "--invert");
            if a.opts.contains_key("--threshold") {
                let t = num(&a, "--threshold", 128.0)?;
                if !(0.0..=255.0).contains(&t) {
                    return Err("--threshold must be 0..255".into());
                }
                o.threshold = Some(t as u8);
            }
            o.quant = match a.opts.get("--quant").map(String::as_str).unwrap_or("kmeans") {
                "kmeans" => autotrace::Quant::KMeans,
                "mediancut" => autotrace::Quant::MedianCut,
                q => return Err(format!("--quant must be kmeans or mediancut, not '{q}'")),
            };
            o.fit = match a.opts.get("--fit").map(String::as_str).unwrap_or("bezier") {
                "bezier" => autotrace::Fit::Bezier,
                "polygon" => autotrace::Fit::Polygon,
                "vw" => autotrace::Fit::PolygonVw,
                f => return Err(format!("--fit must be bezier, polygon or vw, not '{f}'")),
            };
            o.zorder = match a.opts.get("--zorder").map(String::as_str).unwrap_or("area") {
                "area" => autotrace::ZOrder::Area,
                "luminance" => autotrace::ZOrder::Luminance,
                z => return Err(format!("--zorder must be area or luminance, not '{z}'")),
            };
            let res = autotrace::trace(&img, &o)?;
            let fid = autotrace::measure(&img, &res)?;
            if let Some(png) = a.opts.get("--png") {
                let eng = Engine::new(autotrace::to_project(&res, "preview")).map_err(|e| e.join("; "))?;
                let bytes = armatura_core::raster::render_png(&eng, 0.0, &Default::default())?;
                fwrite(png, bytes)?;
            }
            if out.ends_with(".svg") {
                if a.opts.contains_key("--into") {
                    return Err("--into needs a .json output".into());
                }
                fwrite(out, autotrace::to_svg(&res))?;
            } else {
                let mut p = match a.opts.get("--into") {
                    Some(path) => load_project(path)?,
                    None => Project::new(Path::new(image).file_stem().and_then(|s| s.to_str()).unwrap_or("traced"), img.w as f32, img.h as f32, 24.0, 24),
                };
                let name = a.opts.get("--name").map(String::as_str).unwrap_or("trace");
                let used = autotrace::append_to_project(&mut p, &res, name, a.flags.iter().any(|f| f == "--fit-stage"))?;
                Engine::new(p.clone()).map_err(|errs| errs.join("; "))?; // asset dir = the --into project's (load_project)
                write_json(&p, out)?;
                println!("layer group '{used}' added");
            }
            println!("wrote {out}: {} regions, {} strokes, {} paths, {} anchors", res.regions.len(), res.strokes.len(), res.path_count(), res.anchor_count());
            println!("fidelity vs source: MAE {:.2}/255, RMSE {:.2}, PSNR {:.1} dB, SSIM {:.3}", fid.mae, fid.rmse, fid.psnr, fid.ssim);
            Ok(())
        }
        ["trace", image, out] => {
            let bytes = fs::read(image).map_err(|e| format!("cannot read {image}: {e}"))?;
            let cs = trace::trace_png(&bytes, num(&a, "--threshold", 128.0)? as u8, num(&a, "--tolerance", 1.0)?, num(&a, "--min-area", 12.0)?)?;
            let pm = tiny_skia::Pixmap::decode_png(&bytes).map_err(|e| e.to_string())?;
            let mut p = Project::new("traced", pm.width() as f32, pm.height() as f32, 24.0, 24);
            let fill = a.opts.get("--fill").map(String::as_str).unwrap_or("#000000");
            p.layers = vec![trace::contours_to_layer("traced", &cs, fill, armatura_core::math::Vec2::ZERO)];
            Engine::new(p.clone()).map_err(|errs| errs.join("; "))?;
            write_json(&p, out)?;
            println!("wrote {out} ({} contours)", cs.len());
            Ok(())
        }
        ["skin-diffuse", path, layer, out] => {
            let mut p = load_project(path)?;
            p.auto_skin_diffuse(layer, num(&a, "--influences", 4.0)? as usize, weights::DiffusionOptions::default())?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["drive", path, csv, maps, out] => {
            let mut p = load_project(path)?;
            let sig = perf::parse_signals_csv(&fs::read_to_string(csv).map_err(|e| format!("cannot read {csv}: {e}"))?)?;
            let maps: Vec<perf::InputMap> = read_json(maps)?;
            let ch = perf::drive(&maps, &sig, num(&a, "--tol", 0.5)?)?;
            let n = ch.len();
            p.channels.extend(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (+{n} channels)");
            Ok(())
        }
        ["state-bake", path, machine, events, out] => {
            let mut p = load_project(path)?;
            let sm: statemachine::StateMachine = read_json(machine)?;
            let ev: Vec<statemachine::InputEvent> = read_json(events)?;
            let timeline = statemachine::simulate(&sm, &ev, p.frames)?;
            let ch = statemachine::bake(&sm, &timeline, num(&a, "--tol", 0.01)?);
            let n = ch.len();
            p.channels.extend(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (+{n} channels)");
            Ok(())
        }
        ["pose-match", path, params, goals, out] => {
            let mut p = load_project(path)?;
            let params: Vec<posematch::Param> = read_json(params)?;
            let goals: Vec<posematch::Goal> = read_json(goals)?;
            let d = posematch::MatchOptions::default();
            let gens = num(&a, "--generations", d.generations as f32)?;
            if !(1.0..=100_000.0).contains(&gens) {
                return Err("--generations must be between 1 and 100000".into());
            }
            let opts = posematch::MatchOptions {
                generations: gens as usize,
                population: d.population,
                seed: num(&a, "--seed", d.seed as f32)? as u64,
            };
            let r = posematch::pose_match(&p, &params, &goals, &opts)?;
            p.channels.extend(r.channels);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (loss {:.3} px²)", r.loss);
            Ok(())
        }
        ["retarget", src, dst, out] => {
            let sp = load_project(src)?;
            let mut dp = load_project(dst)?;
            let pairs: Vec<(String, String)> = a
                .opts
                .get("--pairs")
                .ok_or("--pairs srcChain=dstChain[,..] is required")?
                .split(',')
                .map(|s| s.split_once('=').map(|(x, y)| (x.trim().to_string(), y.trim().to_string())).ok_or_else(|| format!("bad pair '{s}'")))
                .collect::<Result<_, _>>()?;
            let (ch, notes) = perf::retarget_effectors(&sp.channels, &sp, &dp, &pairs)?;
            let n = ch.len();
            dp.channels.extend(ch);
            compile(dp.clone(), dst)?;
            write_json(&dp, out)?;
            for note in notes {
                println!("note: {note}");
            }
            println!("wrote {out} (+{n} channels)");
            Ok(())
        }
        ["stats", path] => {
            let p = load_project(path)?;
            println!("{}", serde_json::to_string_pretty(&inspect::stats(&p)).unwrap());
            Ok(())
        }
        ["diff", a_path, b_path] => {
            let d = inspect::diff(&load_project(a_path)?, &load_project(b_path)?);
            if d.is_empty() {
                println!("no differences");
                Ok(())
            } else {
                println!("{}", d.join("\n"));
                Err(format!("{} difference(s)", d.len()))
            }
        }
        ["golden", mode, path, gpath] => {
            let e = load(path)?;
            match *mode {
                "write" => {
                    // goldens are regression thumbnails of the CPU raster: a scale above 1 is refused (it used to be clamped silently)
                    if a.scale > 1.0 {
                        return Err(format!("golden write: --scale {} is above 1; goldens are recorded at a scale in (0, 1] (nothing was written)", a.scale));
                    }
                    let g = inspect::golden_record(&e, a.scale);
                    fwrite(gpath, serde_json::to_string(&g).unwrap()).map_err(|e| e.to_string())?;
                    println!("wrote {gpath} ({} frames)", g.frames.len());
                    Ok(())
                }
                "check" => {
                    let g: inspect::Golden = read_json(gpath)?;
                    // exact by default (the CPU rasteriser is deterministic); --tol T lets frames whose 16x16 thumbnail differs by at most T pass
                    let tol = num(&a, "--tol", 0.0)?;
                    if !tol.is_finite() || tol < 0.0 {
                        return Err(format!("--tol needs a number >= 0 (0 = exact), got '{tol}'"));
                    }
                    let bad = inspect::golden_check(&e, &g, tol);
                    if bad.is_empty() {
                        println!("all {} frames match{}", g.frames.len(), if tol > 0.0 { format!(" (within thumbnail tolerance {tol})") } else { " exactly".to_string() });
                        Ok(())
                    } else {
                        for b in &bad {
                            if b.missing {
                                println!("frame {} exists on one side only (project has {} frames, golden {})", b.frame, e.project.frames, g.frames.len());
                            } else {
                                println!("frame {} differs: pixel hash changed, thumbnail distance {:.4}, {} of 256 thumbnail cells differ, max channel difference {}/255", b.frame, b.distance, b.cells_differing, b.max_diff);
                            }
                        }
                        Err(format!("{} frame(s) differ (exact comparison{}; --tol T accepts small differences)", bad.len(), if tol > 0.0 { format!(", tolerance {tol}") } else { String::new() }))
                    }
                }
                _ => Err("golden needs 'write' or 'check'".into()),
            }
        }
        ["lint", path] => {
            let p = load_project(path)?;
            for w in unknown_field_warnings(path) {
                println!("Warning unknown-field [{path}] {w} (ignored)");
            }
            let f = lint::lint(&p, Some(&asset_dir(path)));
            for x in &f {
                println!("{:?} {} [{}] {}", x.severity, x.code, x.target, x.message);
            }
            let mut errors = f.iter().filter(|x| x.severity == lint::Severity::Error).count();
            let mut total = f.len();
            // the same unusable-sound-track check as `check` (a missing, empty or oversized file, a bad format)
            for s in &p.sounds {
                if let Err(err) = soundtrack::check_file(&asset_dir(path), &s.file) {
                    println!("Error sound-unusable [{}] {err}", s.name);
                    errors += 1;
                    total += 1;
                }
            }
            println!("{total} finding(s), {errors} error(s)");
            if errors > 0 { Err("lint errors".into()) } else { Ok(()) }
        }
        ["contracts", path, cpath] => {
            let e = load(path)?;
            let contracts: Vec<lint::Contract> = read_json(cpath)?;
            let v = lint::check_contracts(&e, &contracts)?;
            if v.is_empty() {
                println!("all {} contract(s) hold", contracts.len());
                Ok(())
            } else {
                for x in &v {
                    println!("contract {} violated at frame {}: {}", x.contract, x.frame, x.message);
                }
                Err(format!("{} violation(s)", v.len()))
            }
        }
        ["loop-solve", path, out] => {
            let mut p = load_project(path)?;
            let r = lint::solve_loop_mode(&mut p, num(&a, "--window", 24.0)?, if a.flags.iter().any(|f| f == "--inclusive") { looper::LoopMode::Inclusive } else { looper::LoopMode::Exclusive })?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}: {} channel(s) adjusted, seam {:.3} -> {:.3}", r.channels_adjusted, r.seam_before, r.seam_after);
            Ok(())
        }
        ["coach", path] => {
            let adv = lint::coach(&load_project(path)?);
            if adv.is_empty() {
                println!("no suggestions");
            }
            for x in &adv {
                println!("{} [{}] frame {}: {}", x.code, x.target, x.frame, x.message);
            }
            Ok(())
        }
        ["timecode", frame] => {
            let fps: f64 = match a.opts.get("--fps") {
                None => 24.0,
                Some(v) => v.parse().map_err(|_| format!("--fps needs a number, got '{v}'"))?,
            };
            let drop = a.flags.iter().any(|f| f == "--drop");
            let nodrop = a.flags.iter().any(|f| f == "--nodrop");
            if drop && nodrop {
                return Err("--drop and --nodrop exclude each other".into());
            }
            let mode = if drop { keyops::TcMode::Drop } else if nodrop { keyops::TcMode::NoDrop } else { keyops::TcMode::Auto };
            if frame.contains(':') || frame.contains(';') {
                println!("{}", keyops::parse_timecode_mode(frame, fps, mode)?);
            } else {
                let f: u32 = frame.parse().map_err(|_| format!("bad frame '{frame}'"))?;
                println!("{}", keyops::timecode_mode(f, fps, mode)?);
            }
            Ok(())
        }
        ["keys", op, path, out] => {
            let mut p = load_project(path)?;
            let (from, to) = (num(&a, "--from", f32::MIN)?, num(&a, "--to", f32::MAX)?);
            let want_target = a.opts.get("--target");
            let want_prop: Option<Prop> = match a.opts.get("--prop") {
                Some(s) => Some(serde_json::from_str(&format!("\"{s}\"")).map_err(|_| format!("unknown property '{s}'"))?),
                None => None,
            };
            let ease: Option<armatura_core::anim::Ease> = match a.opts.get("--ease") {
                Some(s) => Some(serde_json::from_str(&format!("{{\"type\":\"{s}\"}}")).map_err(|_| format!("unknown ease '{s}'"))?),
                None => None,
            };
            let by = a.opts.get("--by").map(|_| num(&a, "--by", 0.0)).transpose()?;
            let factor = a.opts.get("--factor").map(|_| num(&a, "--factor", 1.0)).transpose()?;
            if !["shift", "scale", "ease", "reverse"].contains(op) {
                return Err(format!("unknown keys operation '{op}' (shift, scale, ease, reverse)"));
            }
            match *op {
                "shift" if by.is_none() => return Err("keys shift needs --by N".into()),
                "scale" if factor.is_none() => return Err("keys scale needs --factor X".into()),
                "ease" if ease.is_none() => return Err("--ease is required".into()),
                _ => {}
            }
            let mut touched = 0;
            let locked_view = p.clone();
            for c in &mut p.channels {
                if want_target.is_some_and(|t| &c.target != t) || want_prop.is_some_and(|pr| c.prop != pr) {
                    continue;
                }
                let Source::Keys { track } = &mut c.source else { continue };
                if chanmeta::is_locked(&locked_view, &c.target, c.prop) {
                    println!("skipped locked channel {}.{:?}", c.target, c.prop);
                    continue;
                }
                touched += match *op {
                    "shift" => keyops::shift_keys(track, from, to, by.ok_or("keys shift needs --by N")?),
                    "scale" => keyops::scale_keys(track, from, to, num(&a, "--pivot", 0.0)?, factor.ok_or("keys scale needs --factor X")?)?,
                    "ease" => keyops::set_ease(track, from, to, ease.ok_or("--ease is required")?),
                    "reverse" => keyops::reverse_keys(track, from, to),
                    other => return Err(format!("unknown keys operation '{other}' (shift, scale, ease, reverse)")),
                };
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} ({touched} key(s) changed)");
            Ok(())
        }
        ["marker", path, out, name, frame] => {
            let mut p = load_project(path)?;
            let f: f32 = frame.parse().map_err(|_| format!("bad frame '{frame}'"))?;
            let end = match a.opts.get("--end") {
                Some(v) => Some(v.parse::<f32>().map_err(|_| format!("bad --end '{v}'"))?),
                None => None,
            };
            p.markers.retain(|m| m.name != *name);
            p.markers.push(keyops::Marker { name: name.to_string(), frame: f, end });
            p.markers.sort_by(|x, y| x.frame.total_cmp(&y.frame));
            write_json(&p, out)?;
            println!("wrote {out} (marker '{name}' at {} = {})", f, keyops::timecode(f as u32, p.fps).unwrap_or_else(|e| e));
            Ok(())
        }
        ["idle", kind, path, out] => {
            let mut p = load_project(path)?;
            let target = a.opts.get("--target").ok_or("--target is required")?;
            let prop: Prop = {
                let s = a.opts.get("--prop").ok_or("--prop is required")?;
                serde_json::from_str(&format!("\"{s}\"")).map_err(|_| format!("unknown property '{s}'"))?
            };
            let ch = match *kind {
                "blink" => {
                    let d = procgen::BlinkOptions::default();
                    let o = procgen::BlinkOptions {
                        mean_interval_s: num(&a, "--interval", d.mean_interval_s)?,
                        jitter: num(&a, "--jitter", d.jitter)?,
                        closed_frames: num(&a, "--closed", d.closed_frames)?,
                        seed: num(&a, "--seed", d.seed as f32)? as u64,
                        ..d
                    };
                    procgen::blink_channel(target, prop, p.frames, p.fps, &o)?
                }
                "breath" => procgen::breath_channel(target, prop, num(&a, "--amp", 0.03)?, num(&a, "--period", p.fps * 3.0)?)?,
                "sway" => procgen::sway_channel(target, prop, num(&a, "--amp", 2.0)?, num(&a, "--freq", 0.3)?, num(&a, "--seed", 1.0)? as u32),
                other => return Err(format!("unknown generator '{other}' (blink, breath, sway)")),
            };
            p.channels.push(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["follow", path, out] => {
            let mut p = load_project(path)?;
            let target = a.opts.get("--target").ok_or("--target is required")?;
            let prop: Prop = {
                let s = a.opts.get("--prop").ok_or("--prop is required")?;
                serde_json::from_str(&format!("\"{s}\"")).map_err(|_| format!("unknown property '{s}'"))?
            };
            let src = p.channels.iter().find(|c| &c.target == target && c.prop == prop).cloned().ok_or("no such source channel")?;
            let bones: Vec<String> = a.opts.get("--chain").ok_or("--chain a,b,c is required")?.split(',').map(|s| s.trim().to_string()).collect();
            let chs = procgen::follow_through_within(&src, &bones, num(&a, "--delay", 3.0)?, num(&a, "--decay", 0.7)?, Some(p.frames as f32))?;
            let n = chs.len();
            p.channels.extend(chs);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (+{n} channels)");
            Ok(())
        }
        ["import-svg", file, out] => {
            let text = fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?;
            let r = importers::import_svg(&text)?;
            let name = Path::new(file).file_stem().and_then(|n| n.to_str()).unwrap_or("svg");
            let (w, h) = if r.width > 0.0 && r.height > 0.0 { (r.width, r.height) } else { (512.0, 512.0) };
            let mut p = Project::new(name, w, h, 24.0, 24);
            p.layers = vec![armatura_core::project::Layer::vector("svg", r.paths.clone())];
            Engine::new(p.clone()).map_err(|errs| errs.join("; "))?;
            write_json(&p, out)?;
            for w in &r.warnings {
                println!("warning: {w}");
            }
            println!("wrote {out} ({} paths)", r.paths.len());
            Ok(())
        }
        ["import-bvh", path, bvh, out] => {
            let mut p = load_project(path)?;
            let b = importers::parse_bvh(&fs::read_to_string(bvh).map_err(|e| format!("cannot read {bvh}: {e}"))?)?;
            let pairs: Vec<(String, String)> = a
                .opts
                .get("--joint-map")
                .ok_or("--joint-map Joint=bone[,..] is required")?
                .split(',')
                .map(|s| s.split_once('=').map(|(x, y)| (x.trim().to_string(), y.trim().to_string())).ok_or_else(|| format!("bad pair '{s}'")))
                .collect::<Result<_, _>>()?;
            let axis = a.opts.get("--axis").map(String::as_str).unwrap_or("Zrotation");
            let ch = importers::bvh_to_channels(&b, &pairs, axis, p.fps, p.frames, num(&a, "--tol", 0.5)?)?;
            let n = ch.len();
            p.channels.extend(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (+{n} channels from {} BVH frames)", b.rows.len());
            Ok(())
        }
        ["lipsync-text", path, wav, transcript, layer, out] => {
            let mut p = load_project(path)?;
            let audio = read_wav_file(wav)?;
            let text = fs::read_to_string(transcript).map_err(|e| format!("cannot read {transcript}: {e}"))?;
            let lang = speech::Lang::parse(a.opts.get("--lang").map(|s| s.as_str()).unwrap_or("ro"))?;
            let mapping: [usize; 5] = match a.opts.get("--map") {
                None => armatura_core::audio::default_lipsync_map(armatura_core::audio::switch_children(&p, layer).unwrap_or(5)),
                Some(m) => {
                    let v: Vec<usize> = m.split(',').map(|s| s.trim().parse().map_err(|_| format!("bad --map entry '{s}'"))).collect::<Result<_, _>>()?;
                    v.try_into().map_err(|_| "--map needs exactly 5 numbers (rest,small,open,round,wide)".to_string())?
                }
            };
            let al = speech::align_transcript(&audio, &text, lang, &speech::AlignOptions::default())?;
            let n = speech::apply_lipsync(&mut p, layer, &al, lang, &mapping, &speech::LipOptions::default())?;
            compile(p.clone(), path)?;
            write_words(&a, &al)?;
            write_json(&p, out)?;
            println!("wrote {out} ({} words, {} exposure changes; energy + text alignment, not phoneme recognition)", al.words.len(), n);
            Ok(())
        }
        ["speech-motion", path, wav, transcript, out] => {
            let mut p = load_project(path)?;
            let audio = read_wav_file(wav)?;
            let text = fs::read_to_string(transcript).map_err(|e| format!("cannot read {transcript}: {e}"))?;
            let lang = speech::Lang::parse(a.opts.get("--lang").map(|s| s.as_str()).unwrap_or("ro"))?;
            let mut o = speech::MotionOptions { head: a.opts.get("--head").cloned(), brow: a.opts.get("--brow").cloned(), ..Default::default() };
            let num = |k: &str, d: f32| -> Result<f32, String> { a.opts.get(k).map(|v| v.parse::<f32>().map_err(|_| format!("{k} needs a number"))).transpose().map(|x| x.unwrap_or(d)) };
            o.nod_deg = num("--nod", o.nod_deg)?;
            o.fall_deg = num("--fall", o.fall_deg)?;
            o.tilt_deg = num("--tilt", o.tilt_deg)?;
            o.brow_px = num("--brow-px", o.brow_px)?;
            if let Some(s) = a.opts.get("--seed") { o.seed = s.parse().map_err(|_| "--seed needs a whole number".to_string())?; }
            let al = speech::align_transcript(&audio, &text, lang, &speech::AlignOptions::default())?;
            let ch = speech::speech_motion(&p, &al, &o)?;
            let n = speech::apply_speech_motion(&mut p, ch)?;
            compile(p.clone(), path)?;
            write_words(&a, &al)?;
            write_json(&p, out)?;
            println!("wrote {out} ({n} additive channel(s) from {} words)", al.words.len());
            Ok(())
        }
        ["mouth", path, cues, layer, out] => {
            let mut p = load_project(path)?;
            let text = fs::read_to_string(cues).map_err(|e| format!("cannot read {cues}: {e}"))?;
            let cs = importers::parse_mouth_cues(&text, p.fps)?;
            let mapping: [usize; 5] = match a.opts.get("--map") {
                None => [0, 1, 2, 3, 4],
                Some(m) => {
                    let v: Vec<usize> = m.split(',').map(|s| s.trim().parse().map_err(|_| format!("bad --map entry '{s}'"))).collect::<Result<_, _>>()?;
                    v.try_into().map_err(|_| "--map needs exactly 5 numbers (rest,small,open,round,wide)".to_string())?
                }
            };
            let ch = importers::mouth_channel(layer, &cs, &mapping, a.flags.iter().any(|f| f == "--rhubarb"));
            p.channels.retain(|c| !(c.target == *layer && c.prop == Prop::LayerSwitch && matches!(c.source, Source::Keys { .. })));
            p.channels.push(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} ({} cues)", cs.len());
            Ok(())
        }
        ["tag", path] => {
            let p = load_project(path)?;
            for t in vocab::tag_rig(&p) {
                println!("{:<5} {:<24} -> {:<14} ({:.0}%)", t.kind, t.name, t.role, t.confidence * 100.0);
            }
            let missing = vocab::missing_bone_roles(&p);
            if !missing.is_empty() {
                println!("no bone found for: {}", missing.join(", "));
            }
            Ok(())
        }
        ["autorig", path, out] | ["autorig", path, out, _] => {
            let mut p = load_project(path)?;
            let seed: u64 = pos.get(3).map(|s| s.parse().map_err(|_| "seed must be a number".to_string())).transpose()?.unwrap_or(1);
            let rep = autorig::auto_rig(&mut p, seed)?;
            for c in &rep.created { println!("+ {c}"); }
            for s in &rep.skipped { println!("- {s}"); }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["slider", path, out, name, poses] => {
            let mut p = load_project(path)?;
            let mut list = Vec::new();
            for part in poses.split(';') {
                let (at, vals) = part.split_once(':').ok_or("pose needs at:values")?;
                let at: f32 = at.trim().parse().map_err(|_| "bad pose position".to_string())?;
                let mut values = Vec::new();
                for v in vals.split(',').filter(|v| !v.trim().is_empty()) {
                    let (tp, val) = v.split_once('=').ok_or("value needs target.prop=number")?;
                    let (target, prop) = tp.rsplit_once('.').ok_or("target.prop expected")?;
                    let prop: Prop = serde_json::from_str(&format!("\"{}\"", prop.trim())).map_err(|_| format!("unknown property '{prop}'"))?;
                    values.push((target.trim().to_string(), prop, val.trim().parse::<f32>().map_err(|_| "bad number".to_string())?));
                }
                list.push(drivers::PoseEntry { at, values });
            }
            let ctrl = drivers::slider_driver(&mut p, name, &list)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("controller bone: {ctrl} (animate its offset_x)\nwrote {out}");
            Ok(())
        }
        ["view-swap", path, out, layer, driver, ranges] => {
            let mut p = load_project(path)?;
            let (bone, prop) = driver.rsplit_once('.').ok_or("driver must be bone.prop")?;
            let prop: Prop = serde_json::from_str(&format!("\"{prop}\"")).map_err(|_| format!("unknown property '{prop}'"))?;
            let mut r = Vec::new();
            for part in ranges.split(',') {
                let (a, c) = part.split_once(':').ok_or("range needs from:child")?;
                r.push((a.trim().parse::<f32>().map_err(|_| "bad range start".to_string())?, c.trim().parse::<usize>().map_err(|_| "bad child index".to_string())?));
            }
            drivers::view_swap(&mut p, layer, bone, prop, &r)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["align", path, out, mode, layers] | ["align", path, out, mode, layers, _] => {
            let mut p = load_project(path)?;
            let m = layout::parse_align(mode).ok_or("mode: left right top bottom center-x center-y")?;
            let names: Vec<String> = layers.split(',').map(|s| s.trim().to_string()).collect();
            layout::align(&mut p, &names, m, pos.get(5) == Some(&"stage"))?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["distribute", path, out, axis, how, layers] => {
            let mut p = load_project(path)?;
            let ax = match *axis { "x" => layout::Axis::X, "y" => layout::Axis::Y, _ => return Err("axis: x or y".into()) };
            let h = match *how { "gaps" => layout::Distribute::Gaps, "centers" => layout::Distribute::Centers, _ => return Err("how: gaps or centers".into()) };
            let names: Vec<String> = layers.split(',').map(|s| s.trim().to_string()).collect();
            layout::distribute(&mut p, &names, ax, h)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["trim", path, out, layer, idx, a, b] | ["trim", path, out, layer, idx, a, b, _] => {
            let mut p = load_project(path)?;
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let idx: usize = idx.parse().map_err(|_| "path_index must be a number".to_string())?;
            let off = pos.get(7).map(|s| num(s)).transpose()?.unwrap_or(0.0);
            let l = p.find_layer_mut(layer).ok_or_else(|| format!("no layer '{layer}'"))?;
            let armatura_core::project::LayerKind::Vector { paths } = &mut l.kind else { return Err("not a vector layer".into()) };
            let src = paths.get(idx).ok_or("path_index out of range")?;
            let t = armatura_core::layout::trim_path(src, num(a)?, num(b)?, off, 0.5)?;
            paths[idx] = t;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["shot", path, out, name, x, y, zoom] | ["shot", path, out, name, x, y, zoom, _] => {
            let mut p = load_project(path)?;
            let n = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let rot = pos.get(8).map(|s| n(s)).transpose()?.unwrap_or(0.0);
            camshots::save_shot(&mut p, name, n(x)?, n(y)?, n(zoom)?, rot)?;
            write_json(&p, out)?;
            println!("wrote {out} ({} shots)", p.shots.len());
            Ok(())
        }
        ["camera-seq", path, out, steps] => {
            let mut p = load_project(path)?;
            let mut list = Vec::new();
            for part in steps.split(',') {
                let f: Vec<&str> = part.split(':').collect();
                if f.len() < 2 { return Err("step must be frame:shot[:cut]".into()); }
                list.push(camshots::Step { frame: f[0].parse().map_err(|_| "bad frame".to_string())?, shot: f[1].to_string(), cut: f.get(2) == Some(&"cut") });
            }
            camshots::sequence(&mut p, &list)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["follow", path, out, target, path_layer, a, b] | ["follow", path, out, target, path_layer, a, b, _] => {
            let mut p = load_project(path)?;
            let n = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let src = match &p.find_layer(path_layer).ok_or_else(|| format!("no layer '{path_layer}'"))?.kind {
                armatura_core::project::LayerKind::Vector { paths } => paths.first().cloned().ok_or("layer has no path")?,
                _ => return Err("path layer must be a vector layer".into()),
            };
            let (name, is_bone) = match target.strip_prefix("bone:") { Some(b) => (b, true), None => (*target, false) };
            let o = pathfollow::FollowOptions { start_frame: n(a)?, end_frame: n(b)?, orient: pos.get(8) != Some(&"no-orient"), ..Default::default() };
            let ch = pathfollow::follow_path(&p, name, is_bone, &src, &o)?;
            p.channels.extend(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["take-record", path, lib, name, a, b] | ["take-record", path, lib, name, a, b, _] => {
            let p = load_project(path)?;
            let mut l = if Path::new(lib).exists() { load_library(lib)? } else { motionlib::Library::default() };
            let n = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let only: Vec<String> = pos.get(6).map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default();
            let k = motionlib::record(&mut l, &p, name, n(a)?, n(b)?, &only)?;
            fwrite(lib, l.to_json()).map_err(|e| e.to_string())?;
            println!("recorded {k} tracks into {lib}");
            Ok(())
        }
        ["take-apply", path, out, lib, name, at] | ["take-apply", path, out, lib, name, at, _] | ["take-apply", path, out, lib, name, at, _, _] => {
            let mut p = load_project(path)?;
            let l = load_library(lib)?;
            let take = l.find(name).ok_or_else(|| format!("no take '{name}'"))?;
            let rename: Vec<(String, String)> = match pos.get(6) {
                Some(s) if *s != "-" => parse_take_rename(s, take)?,
                _ => vec![],
            };
            let speed: f32 = pos.get(7).map(|s| s.parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| format!("bad speed '{s}': a positive number such as 1 or 0.5 (nothing was written)"))).transpose()?.unwrap_or(1.0);
            let k = motionlib::apply_blended(&mut p, take, at.parse().map_err(|_| format!("bad frame '{at}'"))?, &rename, speed, num(&a, "--blend", 0.0)?)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("applied {k} tracks; wrote {out}");
            Ok(())
        }
        ["take-list", lib] => {
            let l = load_library(lib)?;
            for t in &l.takes { println!("{} ({} frames, {} tracks)", t.name, t.length, t.tracks.len()); }
            Ok(())
        }
        ["sound", "list", path] => {
            let p = load_project(path)?;
            for s in &p.sounds {
                let mut extra = String::new();
                if s.trim_in > 0.0 || s.trim_out.is_some() { extra += &format!(" trim {}s..{}", s.trim_in, s.trim_out.map(|o| format!("{o}s")).unwrap_or_else(|| "end".into())); }
                if s.fade_in > 0.0 { extra += &format!(" fade-in {}s", s.fade_in); }
                if s.fade_out > 0.0 { extra += &format!(" fade-out {}s", s.fade_out); }
                println!("{}: {} at frame {} gain {}{extra}", s.name, s.file, s.start, s.gain);
            }
            if p.sounds.is_empty() { println!("no sound tracks"); }
            Ok(())
        }
        ["sound", "add", path, out, name, file] | ["sound", "add", path, out, name, file, _] | ["sound", "add", path, out, name, file, _, _] => {
            let mut p = load_project(path)?;
            let start: f32 = pos.get(6).map(|s| s.parse().map_err(|_| "bad start frame".to_string())).transpose()?.unwrap_or(0.0);
            let gain: f32 = pos.get(7).map(|s| s.parse().map_err(|_| "bad gain".to_string())).transpose()?.unwrap_or(1.0);
            soundtrack::check_file(&asset_dir(path), file).map_err(|e| format!("sound add: {e} (nothing was added)"))?;
            let mut t = soundtrack::SoundTrack::new(name, file, start, gain);
            apply_sound_opts(&mut t, &a, p.fps)?;
            soundtrack::add_track(&mut p, t)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["sound", "edit", path, out, name] => {
            let mut p = load_project(path)?;
            let fps = p.fps;
            let mut t = p.sounds.iter().find(|s| s.name == *name).cloned().ok_or_else(|| format!("no sound '{name}'"))?;
            if a.opts.contains_key("--start") { t.start = num(&a, "--start", t.start)?; }
            if a.opts.contains_key("--gain") { t.gain = num(&a, "--gain", t.gain)?; }
            apply_sound_opts(&mut t, &a, fps)?;
            soundtrack::add_track(&mut p, t)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["sound", "remove", path, out, name] => {
            let mut p = load_project(path)?;
            if !soundtrack::remove(&mut p, name) { return Err(format!("no sound '{name}'")); }
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["sound", "mix", path, out] => {
            let e = load(path)?;
            soundio::mix_to_file(&e.project, Path::new(path).parent().unwrap_or(Path::new(".")), Path::new(out))?;
            println!("wrote {out} ({:.2} s)", e.project.frames as f32 / e.project.fps);
            Ok(())
        }
        ["make-loop", path, out] | ["make-loop", path, out, _] | ["make-loop", path, out, _, _] => {
            let mut p = load_project(path)?;
            let only: Vec<String> = pos.get(3).filter(|s| &***s != "all").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
            let props: Vec<Prop> = match pos.get(4) {
                Some(s) => s.split(',').map(|x| serde_json::from_str(&format!("\"{}\"", x.trim())).map_err(|_| format!("unknown property '{x}'"))).collect::<Result<_, _>>()?,
                None => vec![],
            };
            let r = looper::make_loop_mode(&mut p, &only, &props, if a.flags.iter().any(|f| f == "--inclusive") { looper::LoopMode::Inclusive } else { looper::LoopMode::Exclusive })?;
            for (t, pr, d) in &r.converted { println!("{t} {pr:?}: removed drift {d:.2}"); }
            for (t, pr) in &r.flattened { println!("note: {t} {pr:?} is now constant: all of its change was travel, which make-loop removes on purpose (in-place loop); lint reports it as L005; name other targets / props to keep a channel's travel"); }
            for t in &r.skipped_locked { println!("{t}: locked, skipped"); }
            if r.converted.is_empty() { println!("no keyed channel converted"); }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["foot-lock", path, out, bone, ground] | ["foot-lock", path, out, bone, ground, _] | ["foot-lock", path, out, bone, ground, _, _] => {
            let mut p = load_project(path)?;
            let mut o = footlock::LockOptions { ground_y: ground.parse().map_err(|_| "bad ground_y".to_string())?, ..Default::default() };
            if let Some(t) = pos.get(6) { o.tolerance = t.parse().map_err(|_| "bad tolerance".to_string())?; }
            if let Some(m) = pos.get(7) { o.min_frames = m.parse().map_err(|_| "bad min_frames".to_string())?; }
            let c = footlock::lock_feet(&mut p, bone, &o)?;
            for k in &c { println!("contact frames {}..{} pinned at ({:.1}, {:.1})", k.start, k.end, k.x, k.y); }
            if c.is_empty() { println!("no ground contact found; nothing changed"); }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["stretch", path, out, follower, leader] | ["stretch", path, out, follower, leader, _] | ["stretch", path, out, follower, leader, _, _] => {
            let mut p = load_project(path)?;
            let mut o = armatura_core::stretch::StretchOptions::default();
            if let Some(v) = pos.get(5) { o.min_factor = v.parse().map_err(|_| "bad min".to_string())?; }
            if let Some(v) = pos.get(6) { o.max_factor = v.parse().map_err(|_| "bad max".to_string())?; }
            let n = armatura_core::stretch::bake_stretch(&mut p, follower, leader, &o)?;
            if n == 0 { println!("already reaching the leader; nothing changed"); } else { println!("corrected {n} frames"); }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["magnet", path, out, ctrl, target, radius, strength] => {
            let mut p = load_project(path)?;
            let t = match target.strip_prefix("bone:") {
                Some(b) => magnet::Target::Bone(b.to_string()),
                None => {
                    let (x, y) = target.split_once(',').ok_or("target is bone:NAME or x,y")?;
                    magnet::Target::Point(armatura_core::math::Vec2::new(x.parse().map_err(|_| "bad x".to_string())?, y.parse().map_err(|_| "bad y".to_string())?))
                }
            };
            let m = magnet::Magnet { radius: radius.parse().map_err(|_| "bad radius".to_string())?, strength: strength.parse().map_err(|_| "bad strength".to_string())? };
            let n = magnet::bake_magnet(&mut p, ctrl, &t, &m)?;
            println!("pulled on {n} frames");
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["physics", path, out, chain, rest @ ..] => {
            let mut p = load_project(path)?;
            let mut o = physics::PhysicsOptions::default();
            let mut overwrite = false;
            let mut lateral = 0.0f32;
            if let Some(pr) = rest.iter().find_map(|kv| kv.strip_prefix("preset=")) {
                o = physics::preset(pr).ok_or_else(|| format!("unknown preset '{pr}' (try {})", physics::PRESETS.join(", ")))?;
            }
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let split_follow = |c: &str| match c.split_once('@') { Some((a, b)) => (a.to_string(), Some(b.to_string())), None => (c.to_string(), None) };
            let mut follow_all: Option<String> = None;
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "gravity" => o.gravity = armatura_core::math::Vec2::new(0.0, num(v)?),
                    "wind" => { let (x, y) = v.split_once(',').ok_or("wind is x,y")?; o.wind = armatura_core::math::Vec2::new(num(x)?, num(y)?); }
                    "preset" => {}
                    "lateral" => lateral = num(v)?,
                    "merge" => overwrite = match v { "keep" => false, "replace" => true, _ => return Err("merge is keep or replace".into()) },
                    "gust" => o.gust = num(v)?,
                    "damping" => o.damping = num(v)?,
                    "circle" => {
                        for c in v.split(';') {
                            let (c, fb) = split_follow(c);
                            let f: Vec<&str> = c.split(',').collect();
                            if f.len() != 3 { return Err("circle is x,y,r[@bone]".into()); }
                            o.colliders.push(physics::Circle { centre: armatura_core::math::Vec2::new(num(f[0])?, num(f[1])?), radius: num(f[2])?, follow: fb });
                        }
                    }
                    "capsule" => {
                        for c in v.split(';') {
                            let (c, fb) = split_follow(c);
                            let f: Vec<&str> = c.split(',').collect();
                            if f.len() != 5 { return Err("capsule is x1,y1,x2,y2,r[@bone]".into()); }
                            o.capsules.push(physics::Capsule { a: armatura_core::math::Vec2::new(num(f[0])?, num(f[1])?), b: armatura_core::math::Vec2::new(num(f[2])?, num(f[3])?), radius: num(f[4])?, follow: fb });
                        }
                    }
                    "follow" => follow_all = Some(v.to_string()),
                    "drive" => {
                        for d in v.split(';') {
                            let (b, g) = d.rsplit_once(':').ok_or("drive is bone:gain")?;
                            o.drives.push(physics::Drive { bone: b.to_string(), gain: num(g)? });
                        }
                    }
                    "vortex" => {
                        for d in v.split(';') {
                            let f: Vec<&str> = d.split(':').collect();
                            if f.len() < 2 || f.len() > 4 { return Err("vortex is bone|x,y:swirl[:radial[:radius]]".into()); }
                            let (centre, follow) = match f[0].split_once(',') {
                                Some((x, y)) => (armatura_core::math::Vec2::new(num(x)?, num(y)?), None),
                                None => (armatura_core::math::Vec2::ZERO, Some(f[0].to_string())),
                            };
                            o.vortices.push(physics::Vortex { centre, follow, swirl: num(f[1])?, radial: if f.len() > 2 { num(f[2])? } else { 0.0 }, radius: if f.len() > 3 { num(f[3])? } else { 200.0 } });
                        }
                    }
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            if let Some(fb) = follow_all {
                for c in o.colliders.iter_mut().filter(|c| c.follow.is_none()) { c.follow = Some(fb.clone()); }
                for c in o.capsules.iter_mut().filter(|c| c.follow.is_none()) { c.follow = Some(fb.clone()); }
            }
            let strands: Vec<Vec<String>> = chain.split('/').map(|c| c.split(',').map(|s| s.trim().to_string()).collect()).collect();
            let ch = physics::simulate_strip(&p, &strands, lateral, &o)?;
            let (added, skipped) = physics::merge_channels(&mut p, ch, overwrite);
            for b in &skipped { println!("kept existing animation on '{b}' (use merge=replace to overwrite)"); }
            if added == 0 && !a.flags.iter().any(|f| f == "--allow-empty") {
                let why = if skipped.is_empty() { "the simulation produced no channels".to_string() } else { format!("{} bone(s) already have animation and merge=keep (the default) leaves it alone", skipped.len()) };
                return Err(format!("physics: nothing baked - {why}; nothing was written. Use merge=replace to overwrite the existing animation, or --allow-empty to accept an unchanged file"));
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("baked {added} bones; wrote {out}");
            Ok(())
        }
        ["cloth", path, out, layer, rest @ ..] => {
            let mut p = load_project(path)?;
            let mut o = cloth::ClothOptions::default();
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let int = |s: &str| s.parse::<u32>().map_err(|_| format!("bad whole number '{s}'"));
            let split_follow = |c: &str| match c.split_once('@') { Some((a, b)) => (a.to_string(), Some(b.to_string())), None => (c.to_string(), None) };
            let v2 = |s: &str, what: &str| -> Result<armatura_core::math::Vec2, String> { let (x, y) = s.split_once(',').ok_or(format!("{what} is x,y"))?; Ok(armatura_core::math::Vec2::new(num(x)?, num(y)?)) };
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "pin" => o.pins = cloth::parse_pins(v)?,
                    "gravity" => o.gravity = armatura_core::math::Vec2::new(0.0, num(v)?),
                    "wind" => o.wind = v2(v, "wind")?,
                    "gust" => o.gust = num(v)?,
                    "gust-hz" => o.gust_hz = num(v)?,
                    "gust-len" => o.gust_len = num(v)?,
                    "stiffness" => o.stiffness = num(v)?,
                    "shear" => o.shear = num(v)?,
                    "bend" => o.bend = num(v)?,
                    "max-stretch" => o.max_stretch = num(v)?,
                    "damping" => o.damping = num(v)?,
                    "cols" => o.cols = Some(int(v)? as usize),
                    "rows" => o.rows = Some(int(v)? as usize),
                    "rect" => { let f: Vec<&str> = v.split(',').collect(); if f.len() != 4 { return Err("rect is x,y,w,h".into()); } o.rect = Some([num(f[0])?, num(f[1])?, num(f[2])?, num(f[3])?]); }
                    "name" => o.name = v.to_string(),
                    "substeps" => o.substeps = int(v)?,
                    "iterations" => o.iterations = int(v)?,
                    "preroll" => o.preroll = int(v)?,
                    "from" => o.from = Some(int(v)?),
                    "to" => o.to = Some(int(v)?),
                    "circle" => {
                        for c in v.split(';') {
                            let (c, fb) = split_follow(c);
                            let f: Vec<&str> = c.split(',').collect();
                            if f.len() != 3 { return Err("circle is x,y,r[@bone]".into()); }
                            o.colliders.push(physics::Circle { centre: armatura_core::math::Vec2::new(num(f[0])?, num(f[1])?), radius: num(f[2])?, follow: fb });
                        }
                    }
                    "capsule" => {
                        for c in v.split(';') {
                            let (c, fb) = split_follow(c);
                            let f: Vec<&str> = c.split(',').collect();
                            if f.len() != 5 { return Err("capsule is x1,y1,x2,y2,r[@bone]".into()); }
                            o.capsules.push(physics::Capsule { a: armatura_core::math::Vec2::new(num(f[0])?, num(f[1])?), b: armatura_core::math::Vec2::new(num(f[2])?, num(f[3])?), radius: num(f[4])?, follow: fb });
                        }
                    }
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            if !rest.iter().any(|kv| kv.starts_with("pin=")) { o.pins = cloth::parse_pins("top")?; }
            p = cloth::bake_cloth(&p, layer, &o)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("baked cloth on '{layer}' ({} channels); wrote {out}", p.channels.iter().filter(|c| c.target.contains("::") && matches!(c.prop, Prop::LatticeDx | Prop::LatticeDy)).count());
            Ok(())
        }
        ["eye-clip", path, out] => {
            let mut p = load_project(path)?;
            let r = eyeclip::auto_eye_clip(&mut p);
            for c in &r.clipped { println!("+ {c}"); }
            for s in &r.skipped { println!("- {s}"); }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["parallax", path, out, layers, far, near] => {
            let mut p = load_project(path)?;
            let names: Vec<String> = layers.split(',').map(|s| s.trim().to_string()).collect();
            depth::spread_parallax(&mut p, &names, far.parse().map_err(|_| "bad far value".to_string())?, near.parse().map_err(|_| "bad near value".to_string())?)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["chan-meta", path, out, target, prop, rest @ ..] => {
            let mut p = load_project(path)?;
            let pr = if *prop == "*" { None } else { Some(Prop::parse(prop).ok_or_else(|| format!("unknown property '{prop}'"))?) };
            let onoff = |v: &str| match v { "on" => Ok(true), "off" => Ok(false), _ => Err("use on or off".to_string()) };
            let mut c = chanmeta::Change::default();
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "color" => c.color = Some(v.to_string()),
                    "shy" => c.shy = Some(onoff(v)?),
                    "solo" => c.solo = Some(onoff(v)?),
                    "lock" => c.lock = Some(onoff(v)?),
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            chanmeta::set_meta(&mut p, target, pr, &c)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["reference", path, out, layer, state] => {
            let mut p = load_project(path)?;
            let on = match *state { "on" => true, "off" => false, _ => return Err("use on or off".into()) };
            p.find_layer_mut(layer).ok_or_else(|| format!("no layer '{layer}'"))?.reference = on;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["import-anim", path, out, name, file, rest @ ..] => {
            let mut p = load_project(path)?;
            let (mut looped, mut start, mut x, mut y) = (false, 0.0f32, 0.0f32, 0.0f32);
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "loop" => looped = v == "on",
                    "start" => start = num(v)?,
                    "x" => x = num(v)?,
                    "y" => y = num(v)?,
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let bytes = fs::read(file).map_err(|e| format!("{file}: {e}"))?;
            let n = armatura_core::animimport::import_animation(&mut p, name, &bytes, armatura_core::math::Vec2::new(x, y), start, looped)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("imported {n} frames as layer '{name}'; wrote {out}");
            Ok(())
        }
        ["dof", path, frame, out] => {
            let e = load(path)?;
            let f: f32 = frame.parse().map_err(|_| "bad frame".to_string())?;
            let f = check_frame_in_range(f, e.project.frames, a.flags.iter().any(|f| f == "--clamp"))?;
            let d = armatura_core::dof::DofOptions { scale: a.scale, focus: num(&a, "--focus", 1.0)?, strength: num(&a, "--strength", 12.0)?, max_radius: num(&a, "--max-radius", 24.0)? };
            let (w, h, rgba) = armatura_core::dof::render_dof_animated(&e.project, f, &d, a.opts.contains_key("--focus"), a.opts.contains_key("--strength"))?;
            fwrite(out, open_formats::encode_rgba_png(w, h, &rgba)?).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        ["dof-export", path, out] => {
            let e = load(path)?;
            let d = armatura_core::dof::DofOptions { scale: a.scale, focus: num(&a, "--focus", 1.0)?, strength: num(&a, "--strength", 12.0)?, max_radius: num(&a, "--max-radius", 24.0)? };
            let whole = |k: &str, v: f32| -> Result<u32, String> {
                if v.is_finite() && v >= 0.0 && v.fract() == 0.0 { Ok(v as u32) } else { Err(format!("{k} needs a whole frame number >= 0")) }
            };
            let from = whole("--from", num(&a, "--from", 0.0)?)?;
            let to = if a.opts.contains_key("--to") { Some(whole("--to", num(&a, "--to", 0.0)?)?) } else { None };
            let (ff, fs_) = (a.opts.contains_key("--focus"), a.opts.contains_key("--strength"));
            dof_export_preflight(out, &a)?;
            let lower = out.to_lowercase();
            if lower.ends_with(".gif") {
                fwrite(out, armatura_core::dof::dof_gif(&e.project, from, to, &d, ff, fs_)?).map_err(|e| e.to_string())?;
            } else if lower.ends_with(".apng") {
                fwrite(out, armatura_core::dof::dof_apng(&e.project, from, to, &d, ff, fs_)?).map_err(|e| e.to_string())?;
            } else if lower.ends_with(".mp4") || lower.ends_with(".webm") {
                let webm = lower.ends_with(".webm");
                let codec = a.opts.get("--codec").map(String::as_str).unwrap_or("vp9");
                if !e.project.sounds.is_empty() {
                    eprintln!("note: dof-export video carries no audio; the project's {} sound track(s) are not included (use export for sound)", e.project.sounds.len());
                }
                let pngs = armatura_core::dof::dof_png_sequence(&e.project, from, to, &d, ff, fs_)?;
                encode_png_video(&pngs, e.project.fps, out, webm, codec)?;
            } else if Path::new(out).extension().is_some() {
                return Err(format!("dof-export writes .gif, .apng, .mp4, .webm or a folder (no extension); '{out}' is none of these (nothing was written)"));
            } else {
                let pngs = armatura_core::dof::dof_png_sequence(&e.project, from, to, &d, ff, fs_)?;
                fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
                for (i, b) in pngs.iter().enumerate() {
                    fwrite(Path::new(out).join(format!("f{:04}.png", from as usize + i)), b).map_err(|e| e.to_string())?;
                }
            }
            println!("wrote {out}");
            Ok(())
        }
        ["import-seq", path, out, name, folder, rest @ ..] => {
            let mut p = load_project(path)?;
            let (mut hold, mut looped, mut start, mut x, mut y) = (1.0f32, false, 0.0f32, 0.0f32, 0.0f32);
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "hold" => hold = num(v)?,
                    "loop" => looped = v == "on",
                    "start" => start = num(v)?,
                    "x" => x = num(v)?,
                    "y" => y = num(v)?,
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let mut names: Vec<String> = fs::read_dir(folder).map_err(|e| format!("{folder}: {e}"))?
                .filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.to_lowercase().ends_with(".png")).collect();
            // natural order: compare digit runs as numbers (frame2 before frame10)
            let key = |s: &String| -> Vec<(u64, String)> {
                let mut out = Vec::new();
                let (mut num_s, mut txt) = (String::new(), String::new());
                for ch in s.chars() {
                    if ch.is_ascii_digit() { if !txt.is_empty() { out.push((0, std::mem::take(&mut txt))); } num_s.push(ch); }
                    else { if !num_s.is_empty() { out.push((num_s.parse().unwrap_or(0), String::new())); num_s.clear(); } txt.push(ch); }
                }
                if !num_s.is_empty() { out.push((num_s.parse().unwrap_or(0), String::new())); }
                if !txt.is_empty() { out.push((0, txt)); }
                out
            };
            names.sort_by_key(|a| key(a));
            if names.is_empty() { return Err(format!("no .png files in {folder}")); }
            let mut frames = Vec::new();
            for n in &names { frames.push((n.clone(), fs::read(Path::new(folder).join(n)).map_err(|e| format!("{n}: {e}"))?)); }
            seqimport::import_png_sequence(&mut p, name, &frames, armatura_core::math::Vec2::new(x, y), start, hold, looped)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("imported {} images; wrote {out}", frames.len());
            Ok(())
        }
        ["render-hdr", path, frame, out] | ["render-hdr", path, frame, out, _] => {
            let e = load(path)?;
            let frame: f32 = frame.parse().map_err(|_| format!("bad frame number '{frame}'"))?;
            let frame = check_frame_in_range(frame, e.project.frames, a.flags.iter().any(|f| f == "--clamp"))?;
            e.evaluate_checked(frame)?;
            let mut prec = match a.opts.get("--precision") { Some(v) => hdr::Precision::parse(v)?, None => hdr::Precision::Half };
            if a.flags.iter().any(|f| f == "--float32") { prec = hdr::Precision::Float; }
            let legacy = a.flags.iter().any(|f| f == "--8bit");
            if legacy && prec == hdr::Precision::Float { return Err("--8bit source cannot be combined with float32 output".into()); }
            armatura_core::raster::check_project_size(&e.project, a.scale)?;
            let ro = RasterOptions { scale: a.scale, show_bones: false, transparent: true };
            let lower = out.to_lowercase();
            let linear = pos.get(4) == Some(&"linear");
            let (w, h, bytes) = if legacy {
                let (w, h, px) = armatura_core::raster::render_rgba(&e, frame, &ro);
                let b = if lower.ends_with(".exr") { hdr::encode_exr_half(w, h, &px)? }
                    else if lower.ends_with(".png") { hdr::encode_png16(w, h, &px, linear)? }
                    else { return Err("output must end in .exr or .png".into()) };
                (w, h, b)
            } else {
                let img = armatura_core::fraster::render_float(&e, frame, &ro);
                let b = if lower.ends_with(".exr") { hdr::encode_exr_buf(img.width, img.height, &img.rgba, prec)? }
                    else if lower.ends_with(".png") { hdr::encode_png16_buf(img.width, img.height, &img.rgba, linear)? }
                    else { return Err("output must end in .exr or .png".into()) };
                (img.width, img.height, b)
            };
            fwrite(out, bytes).map_err(|e| e.to_string())?;
            println!("wrote {out} ({w}x{h})");
            Ok(())
        }
        ["import-lottie", inp, out, rest @ ..] if rest.len() <= 1 => {
            let text = fs::read_to_string(inp).map_err(|e| format!("{inp}: {e}"))?;
            let r = lottie_import::import_lottie(&text)?;
            for w in &r.warnings { println!("warning: {w}"); }
            compile(r.project.clone(), inp)?;
            write_json(&r.project, out)?;
            if let Some(fxp) = rest.first() {
                fwrite(fxp, serde_json::to_string_pretty(&r.post_fx).map_err(|e| e.to_string())?)?;
                println!("wrote {} post effect(s) to {fxp}", r.post_fx.len());
            } else if !r.post_fx.is_empty() {
                println!("note: {} blur / drop shadow effect(s) found; pass a third path to save them as a post-effect chain", r.post_fx.len());
            }
            println!("imported {} layers; wrote {out}", r.project.layers.len());
            Ok(())
        }
        #[cfg(not(feature = "scripting"))]
        ["script", ..] => Err("this build has no scripting support (built without the `scripting` feature)".to_string()),
        #[cfg(feature = "scripting")]
        ["script", path, out, file] => {
            let p = load_project(path)?;
            let src = fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            let mut ed = Editor::with_asset_dir(p, asset_dir(path)).map_err(|errs| format!("{path} is not a valid project:\n  - {}", errs.join("\n  - ")))?;
            let r = armatura_core::script::run_script(&mut ed, &src)?;
            for l in &r.log { println!("{l}"); }
            if r.applied == 0 {
                eprintln!("warning: script: 0 requests were applied (the script changed nothing), so {out} is a copy of the input");
            }
            write_json(ed.project(), out)?;
            println!("{} request(s) applied; wrote {out}", r.applied);
            Ok(())
        }
        ["audio-react", path, out, wav, target, prop, rest @ ..] => {
            let mut p = load_project(path)?;
            let audio = read_wav_file(wav)?;
            let pr = Prop::parse(prop).ok_or_else(|| format!("unknown property '{prop}'"))?;
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            let mut o = audiomap::LevelOptions::default();
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "gain" => o.gain = num(v)?, "offset" => o.offset = num(v)?, "floor" => o.floor = num(v)?,
                    "attack" => o.attack = num(v)?, "release" => o.release = num(v)?, "add" => o.additive = v == "on",
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let ch = audiomap::level_channel(target, pr, &audio, p.fps, p.frames, &o)?;
            chanmeta::assert_unlocked(&p, target, pr)?;
            p.channels.push(ch);
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["beats", path, out, wav, rest @ ..] => {
            let mut p = load_project(path)?;
            let audio = read_wav_file(wav)?;
            let (mut sens, mut gap, mut as_events, mut name) = (1.5f32, 4u32, false, "beat".to_string());
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "sensitivity" => sens = v.parse().map_err(|_| "bad sensitivity".to_string())?,
                    "gap" => gap = v.parse().map_err(|_| "bad gap".to_string())?,
                    "as" => as_events = match v { "events" => true, "markers" => false, _ => return Err("as is markers or events".into()) },
                    "name" => name = v.to_string(),
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let b = audiomap::detect_beats(&audio, p.fps, p.frames, sens, gap)?;
            if as_events { audiomap::beats_to_events(&mut p, &b, &name) } else { audiomap::beats_to_markers(&mut p, &b, &name) }
            write_json(&p, out)?;
            println!("{} beats at frames {:?}; wrote {out}", b.len(), b);
            Ok(())
        }
        ["waveform", wav, out] | ["waveform", wav, out, _] => {
            let audio = read_wav_file(wav)?;
            let n: usize = match pos.get(3) { Some(s) => s.trim_start_matches("buckets=").parse().map_err(|_| "bad bucket count".to_string())?, None => 800 };
            let peaks = audiomap::waveform_peaks(&audio, n)?;
            let json = serde_json::json!({"sample_rate": audio.sample_rate, "duration": audio.duration(), "peaks": peaks});
            fwrite(out, serde_json::to_string(&json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            println!("wrote {out} ({n} buckets)");
            Ok(())
        }
        ["skin", path, out, action, group, rest @ ..] => {
            let mut p = load_project(path)?;
            match (*action, rest) {
                ("add", [name, layers]) => {
                    let ls: Vec<String> = layers.split(',').map(|s| s.trim().to_string()).collect();
                    skins::add_skin(&mut p, group, name, &ls)?;
                }
                ("rig", [name, constraints, iks]) => {
                    let list = |s: &str| -> Vec<String> { if s == "-" { vec![] } else { s.split(',').map(|x| x.trim().to_string()).collect() } };
                    skins::set_rig(&mut p, group, name, &list(constraints), &list(iks))?;
                }
                ("bones", [name, bones]) => {
                    let bs: Vec<String> = if *bones == "-" { vec![] } else { bones.split(',').map(|x| x.trim().to_string()).collect() };
                    skins::set_bones(&mut p, group, name, &bs)?;
                }
                ("apply", [name]) => skins::apply_skin(&mut p, group, name)?,
                ("schedule", [steps]) => {
                    let mut v = Vec::new();
                    for part in steps.split(',') {
                        let (f, n) = part.split_once(':').ok_or("step must be frame:name")?;
                        v.push((f.trim().parse::<f32>().map_err(|_| "bad frame".to_string())?, n.trim().to_string()));
                    }
                    skins::schedule(&mut p, group, &v)?;
                }
                _ => return Err("usage: skin <project> <out> add <group> <name> <layers> | rig <group> <name> <constraints|-> <iks|-> | bones <group> <name> <bones|-> | apply <group> <name> | schedule <group> <frame:name,...>".into()),
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["parent-switch", path, out, bone, steps] => {
            let mut p = load_project(path)?;
            let mut v = Vec::new();
            for part in steps.split(',') {
                let (f, n) = part.split_once(':').ok_or("step must be frame:parent")?;
                v.push(parentswitch::Switch { frame: f.trim().parse().map_err(|_| "bad frame".to_string())?, parent: if n.trim() == "none" { None } else { Some(n.trim().to_string()) } });
            }
            parentswitch::bake_parent_switch(&mut p, bone, &v)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["bone-vis", path, out, bone, from, to] => {
            let mut p = load_project(path)?;
            bonevis::set_range(&mut p, bone, from.parse().map_err(|_| "bad from".to_string())?, to.parse().map_err(|_| "bad to".to_string())?)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["bone-vis", path, out, bone, "clear"] => {
            let mut p = load_project(path)?;
            println!("{}", if bonevis::clear(&mut p, bone) { "range removed" } else { "no range set" });
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["face-calibrate", marks, out] => {
            let (stream, cfg) = face_inputs(&a, marks, None, 30.0)?;
            let n = num(&a, "--calibrate-frames", 30.0)? as usize;
            let cal = armatura_core::faceparam::calibrate(&stream, &cfg, n)?;
            fwrite(out, serde_json::to_string_pretty(&cal).map_err(|e| e.to_string())?)?;
            println!("{} frames, neutral from the first {n}; wrote {out}", stream.frames.len());
            Ok(())
        }
        ["face-bake", path, marks, map, out] => {
            let mut p = load_project(path)?;
            let (stream, cfg) = face_inputs(&a, marks, Some(map), p.fps)?;
            let (mapping, _) = armatura_core::faceparam::parse_mapfile(&fs::read_to_string(map).map_err(|e| format!("cannot read {map}: {e}"))?)?;
            let cal = match (a.opts.get("--calib"), num(&a, "--calibrate-frames", 30.0)? as usize) {
                (Some(f), _) => armatura_core::faceparam::Calibration::from_json(&fs::read_to_string(f).map_err(|e| format!("cannot read {f}: {e}"))?)?,
                (None, 0) => armatura_core::faceparam::Calibration::nominal(),
                (None, n) => armatura_core::faceparam::calibrate(&stream, &cfg, n)?,
            };
            let r = armatura_core::faceparam::bake(&mut p, &stream, &mapping, &cfg, &cal, num(&a, "--tol", 0.01)?)?;
            for n in &r.notes { println!("note: {n}"); }
            check_bake_not_empty(&a, stream.frames.len(), &r, "face-bake", "mapfile 'params' (source \"face\", signal id)")?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("{} landmark frames -> +{} channels ({} keys in them), {} trigger keys set, {} events; wrote {out}", stream.frames.len(), r.channels, r.param_keys, r.keys_set, r.events);
            if r.cut_frames > 0 {
                eprintln!("warning: face-bake: the landmark stream is {} frame(s) longer than the project ({} frames at {} fps); those frames were not baked (the project length is unchanged)", r.cut_frames, p.frames, p.fps);
            }
            Ok(())
        }
        ["live-map", path, events, map, out] => {
            let mut p = load_project(path)?;
            let (ev, warns) = if events.ends_with(".mid") || events.ends_with(".midi") {
                (armatura_core::midi::smf_to_events(&fs::read(events).map_err(|e| format!("cannot read {events}: {e}"))?)?, vec![])
            } else {
                armatura_core::liveinput::parse_recording_json(&fs::read_to_string(events).map_err(|e| format!("cannot read {events}: {e}"))?)?
            };
            for w in &warns { println!("warning: {w}"); }
            let mapping: armatura_core::liveinput::Mapping = read_json(map)?;
            let lib = match a.opts.get("--takes") {
                Some(l) => Some(load_library(l)?),
                None => None,
            };
            let r = armatura_core::liveinput::bake(&mut p, &ev, &mapping, lib.as_ref(), num(&a, "--tol", 0.01)?)?;
            for n in &r.notes { println!("note: {n}"); }
            check_bake_not_empty(&a, ev.len(), &r, "live-map", "mapping.json 'params'/'triggers' (source, id)")?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("{} events -> +{} channels ({} keys in them), {} trigger keys set, {} takes, {} events ({} after the last frame skipped); wrote {out}", ev.len(), r.channels, r.param_keys, r.keys_set, r.takes, r.events, r.skipped);
            if r.cut_frames > 0 {
                eprintln!("warning: live-map: the recording is {} frame(s) longer than the project ({} frames at {} fps); those frames were not baked (the project length is unchanged)", r.cut_frames, p.frames, p.fps);
            }
            Ok(())
        }
        ["clip", "add", path, out, lib, take, name] => {
            let mut p = load_project(path)?;
            let l = load_library(lib)?;
            let list = |k: &str| -> Vec<String> { a.opts.get(k).map(|s| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default() };
            let mut c = clipstack::Clip::new(name, motionlib::Take { name: take.to_string(), length: 1.0, tracks: vec![] });
            c.start = num(&a, "--at", 0.0)?;
            c.speed = num(&a, "--speed", 1.0)?;
            c.loops = a.opts.get("--loops").map(|v| v.parse::<u32>().map_err(|_| format!("--loops needs a whole number, got '{v}'"))).transpose()?.unwrap_or(1);
            c.weight = num(&a, "--weight", 1.0)?;
            c.fade_in = num(&a, "--fade-in", 0.0)?;
            c.fade_out = num(&a, "--fade-out", 0.0)?;
            c.mode = match a.opts.get("--mode").map(String::as_str) {
                None | Some("replace") => clipstack::ClipMode::Replace,
                Some("additive") | Some("add") => clipstack::ClipMode::Additive,
                Some("multiply") => clipstack::ClipMode::Multiply,
                Some(m) => return Err(format!("unknown clip mode '{m}' (replace, additive, multiply)")),
            };
            c.include = list("--include");
            c.exclude = list("--exclude");
            c.rename = list("--rename").iter().filter_map(|kv| kv.split_once('=')).map(|(x, y)| (x.to_string(), y.to_string())).collect();
            for r in list("--track-weight") {
                let (pat, w) = r.split_once('=').ok_or("--track-weight needs pattern=weight")?;
                let (pat, prop) = parse_pattern_prop(pat)?;
                c.track_weights.push(clipstack::TrackWeight { pattern: pat, prop, weight: w.parse().map_err(|_| format!("bad weight '{w}'"))? });
            }
            clipstack::add(&mut p, &l, take, c)?;
            write_json(&p, out)?;
            println!("added clip '{name}' ({} clips); run clip bake to generate channels", p.clips.len());
            Ok(())
        }
        ["clip", "list", path] => {
            let p = load_project(path)?;
            for (i, c) in p.clips.iter().enumerate() {
                let mut extra = String::new();
                if !c.weight_keys.keys.is_empty() { extra += &format!(" weight-keys {}", c.weight_keys.keys.len()); }
                if !c.include.is_empty() { extra += &format!(" include {}", c.include.join(",")); }
                if !c.exclude.is_empty() { extra += &format!(" exclude {}", c.exclude.join(",")); }
                for r in &c.track_weights { extra += &format!(" {}{}={}", r.pattern, r.prop.map(|p| format!(":{p:?}")).unwrap_or_default(), r.weight); }
                println!("{}. {} take '{}' frames {}..{} x{} speed {} {:?} weight {}{extra}", i + 1, c.name, c.take.name, c.start, c.end(), c.loops, c.speed, c.mode, c.weight);
            }
            if p.clips.is_empty() { println!("no clips"); }
            println!("{} baked channels", p.clip_baked.len());
            Ok(())
        }
        ["clip", "remove", path, out, name] => {
            let mut p = load_project(path)?;
            clipstack::remove(&mut p, name)?;
            write_json(&p, out)?;
            println!("removed clip '{name}'; run clip bake to update channels");
            Ok(())
        }
        ["clip", "set-weight", path, out, name] | ["clip", "set-weight", path, out, name, _] => {
            let mut p = load_project(path)?;
            let w: Option<f32> = pos.get(5).map(|s| s.parse().map_err(|_| format!("bad weight '{s}'"))).transpose()?;
            if let Some(k) = a.opts.get("--keys") {
                let mut keys = Vec::new();
                for part in k.split(',') {
                    let (f, v) = part.split_once(':').ok_or("--keys needs frame:weight pairs")?;
                    keys.push((f.parse::<f32>().map_err(|_| format!("bad frame '{f}'"))?, v.parse::<f32>().map_err(|_| format!("bad weight '{v}'"))?));
                }
                clipstack::set_weight_keys(&mut p, name, &keys)?;
            } else if let Some(t) = a.opts.get("--track") {
                let (pat, prop) = parse_pattern_prop(t)?;
                clipstack::set_track_weight(&mut p, name, &pat, prop, w.ok_or("--track needs a weight")?)?;
            } else {
                clipstack::set_weight(&mut p, name, w.ok_or("give a weight or --keys")?)?;
            }
            write_json(&p, out)?;
            println!("updated clip '{name}'; run clip bake to update channels");
            Ok(())
        }
        ["clip", "bake", path, out] => {
            let mut p = load_project(path)?;
            let r = clipstack::bake(&mut p)?;
            write_json(&p, out)?;
            println!("baked {} clips into {} channels ({} replaced); wrote {out}", r.clips, r.channels, r.replaced);
            Ok(())
        }
        ["cage", path, out, layer, rest @ ..] => {
            let mut p = load_project(path)?;
            let mut o = cage::CageOptions::default();
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            for kv in rest {
                let (k, v) = kv.split_once('=').map(|(k, v)| (k, v.to_string())).unwrap_or((&kv[..], String::new()));
                match k {
                    "auto" => o.rect = None,
                    "hull" => o.hull = true,
                    "rect" => {
                        let f: Vec<&str> = v.split(',').collect();
                        if f.len() != 4 { return Err("rect is x,y,w,h".into()); }
                        o.rect = Some([num(f[0])?, num(f[1])?, num(f[2])?, num(f[3])?]);
                    }
                    "verts" => o.verts = v.parse::<usize>().map_err(|_| format!("bad whole number '{v}'"))?,
                    "pad" => o.pad = num(&v)?,
                    "name" => o.name = v,
                    other => return Err(format!("unknown option '{other}' (rect=, auto, hull, verts=, pad=, name=)")),
                }
            }
            p = cage::add_cage(&p, layer, &o)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            let n = match p.find_layer(layer).and_then(|l| l.deformers.iter().find(|d| d.name() == o.name)) {
                Some(armatura_core::project::Deformer::Cage { rest, .. }) => rest.len(),
                _ => 0,
            };
            println!("added cage '{}' ({n} vertices) to '{layer}'; wrote {out}", o.name);
            Ok(())
        }
        ["cage-pose", path, out, layer, frame, rest @ ..] => {
            let p = load_project(path)?;
            let fr: f32 = frame.parse().map_err(|_| format!("bad frame '{frame}'"))?;
            let (mut name, mut moves, mut all, mut ease) = ("cage".to_string(), Vec::new(), None, armatura_core::anim::Ease::Linear);
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("bad option '{kv}' (use name=, move=i:dx,dy;..., all=dx,dy, ease=)"))?;
                match k {
                    "name" => name = v.to_string(),
                    "move" => moves = cage::parse_moves(v)?,
                    "all" => {
                        let (a, b) = v.split_once(',').ok_or("all is dx,dy")?;
                        all = Some((a.trim().parse::<f32>().map_err(|_| format!("bad number '{a}'"))?, b.trim().parse::<f32>().map_err(|_| format!("bad number '{b}'"))?));
                    }
                    "ease" => ease = serde_json::from_str(&format!("{{\"type\":\"{v}\"}}")).map_err(|_| format!("unknown ease '{v}'"))?,
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let q = cage::pose_cage(&p, layer, &name, fr, &moves, all, ease)?;
            compile(q.clone(), path)?;
            write_json(&q, out)?;
            println!("keyed cage '{name}' on '{layer}' at frame {fr}; wrote {out}");
            Ok(())
        }
        ["mesh-trace", path, out, layer, rest @ ..] => {
            let mut p = load_project(path)?;
            let mut o = meshtrace::TraceOptions::default();
            let (mut skin, mut bones_r, mut influences) = (true, None, 4usize);
            let num = |s: &str| s.parse::<f32>().map_err(|_| format!("bad number '{s}'"));
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "density" => o.density = num(v)?,
                    "tolerance" => o.tolerance = num(v)?,
                    "edge" => o.edge_size = num(v)?,
                    "grading" => o.grading = num(v)?,
                    "threshold" => o.threshold = num(v)?.clamp(0.0, 255.0) as u8,
                    "smooth" => o.smooth.iterations = v.parse().map_err(|_| format!("smooth needs a whole number, got '{v}'"))?,
                    "lambda" => o.smooth.lambda = num(v)?,
                    "taubin" => o.smooth.taubin = v != "0",
                    "smooth-boundary" => o.smooth.boundary = v != "0",
                    "hot-size" => o.hotspot_size = num(v)?,
                    "bones" => bones_r = Some(num(v)?),
                    "skin" => skin = v != "0",
                    "influences" => influences = v.parse().map_err(|_| format!("influences needs a whole number, got '{v}'"))?,
                    "hotspot" => {
                        for h in v.split(';').filter(|h| !h.is_empty()) {
                            let f: Vec<&str> = h.split(',').collect();
                            if f.len() != 3 {
                                return Err(format!("hotspot '{h}' must be x,y,r"));
                            }
                            o.hotspots.push(meshtrace::Hotspot { pos: armatura_core::math::Vec2::new(num(f[0])?, num(f[1])?), radius: num(f[2])? });
                        }
                    }
                    _ => return Err(format!("unknown mesh-trace option '{k}'")),
                }
            }
            if let Some(r) = bones_r {
                o.hotspots.extend(meshtrace::bone_hotspots(&p, r)?);
            }
            let st = meshtrace::trace_layer_mesh(&mut p, layer, &o, &asset_dir(path), skin, influences)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!(
                "wrote {out}: {} vertices ({} on the outline), {} triangles, area {:.0} px2\n  quality min {:.3} mean {:.3}, min angle {:.1} deg, mean edge {:.1} px at the outline / {:.1} px inside{}",
                st.vertices, st.boundary_vertices, st.triangles, st.area, st.min_quality, st.mean_quality, st.min_angle_deg, st.mean_boundary_edge, st.mean_interior_edge,
                if st.unresolved_edges > 0 { format!("\n  warning: {} outline edges could not be recovered", st.unresolved_edges) } else { String::new() }
            );
            Ok(())
        }
        ["warp-pivot", path, out, layer, frame, rest @ ..] | ["soft-scale", path, out, layer, frame, rest @ ..] => {
            let soft = pos[0] == "soft-scale";
            let p = load_project(path)?;
            let fr: f32 = frame.parse().map_err(|_| format!("bad frame '{frame}'"))?;
            let num = |s: &str| s.trim().parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("bad number '{s}'"));
            let mut o = warp::WarpOptions { frame: fr, ..Default::default() };
            let (mut resize, mut keep, mut sel_given) = (None, true, false);
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "deformer" => o.deformer = Some(v.to_string()),
                    "verts" => {
                        o.selection.indices = links::parse_indices(v)?;
                        sel_given = true;
                    }
                    "region" => {
                        o.selection.region = Some(warp::parse_region(v)?);
                        sel_given = true;
                    }
                    "pivot" => o.pivot = warp::Pivot::parse(v)?,
                    "rotate" => o.xform.rotate_deg = num(v)?,
                    "scale" => {
                        o.xform.scale = warp::parse_scale(v)?;
                        resize = Some(warp::ResizeTo::Factor(num(v.split(',').next().unwrap_or(v))?));
                    }
                    "move" => {
                        let (a, b) = v.split_once(',').ok_or("move is dx,dy")?;
                        o.xform.translate = armatura_core::math::Vec2::new(num(a)?, num(b)?);
                    }
                    "radius" => o.radius = num(v)?,
                    "falloff" => o.falloff = warp::Falloff::parse(v)?,
                    "width" => resize = Some(warp::ResizeTo::Width(num(v)?)),
                    "height" => resize = Some(warp::ResizeTo::Height(num(v)?)),
                    "size" => {
                        let (a, b) = v.split_once('x').ok_or("size is WxH")?;
                        resize = Some(warp::ResizeTo::Size(num(a)?, num(b)?));
                    }
                    "aspect" => keep = match v { "keep" => true, "free" => false, _ => return Err("aspect is keep or free".into()) },
                    "ease" => o.ease = serde_json::from_str(&format!("{{\"type\":\"{v}\"}}")).map_err(|_| format!("unknown ease '{v}'"))?,
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let q = if soft && !sel_given {
                let to = resize.ok_or("soft-scale without verts=/region= resizes the whole mesh: give scale=, width=, height= or size=")?;
                warp::resize_mesh(&p, layer, o.deformer.as_deref(), to, keep, fr, o.ease)?
            } else {
                warp::warp_pivot(&p, layer, &o)?
            };
            compile(q.clone(), path)?;
            write_json(&q, out)?;
            println!("{} on '{layer}' keyed at frame {fr}; wrote {out}", pos[0]);
            Ok(())
        }
        ["layer-pivot", path, out, layer, rest @ ..] => {
            let p = load_project(path)?;
            let mut piv = None;
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "pivot" => piv = Some(warp::Pivot::parse(v)?),
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let q = warp::set_layer_pivot(&p, layer, &piv.ok_or("layer-pivot needs pivot=x,y or pivot=bone:NAME")?)?;
            compile(q.clone(), path)?;
            write_json(&q, out)?;
            println!("moved the pivot of '{layer}'; wrote {out}");
            Ok(())
        }
        ["mesh-lock", "list", path] => {
            let p = load_project(path)?;
            for l in &p.mesh_locks {
                println!("{}: {}:{} follows {}:{} vertices {} strength {}", l.name, l.follower.layer, l.follower.path, l.host.layer, l.host.path,
                    if l.vertices.is_empty() { "all".to_string() } else { format!("{:?}", l.vertices) }, l.strength);
            }
            println!("{} mesh lock(s)", p.mesh_locks.len());
            Ok(())
        }
        ["mesh-lock", "remove", path, out, name] => {
            let q = links::remove_mesh_lock(&load_project(path)?, name)?;
            write_json(&q, out)?;
            println!("removed mesh lock '{name}'; wrote {out}");
            Ok(())
        }
        ["mesh-lock", "add", path, out, name, rest @ ..] => {
            let p = load_project(path)?;
            let (mut host, mut fol, mut verts, mut strength) = (None, None, Vec::new(), 1.0f32);
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "host" => host = Some(links::parse_geom(v)?),
                    "follower" => fol = Some(links::parse_geom(v)?),
                    "verts" => verts = links::parse_indices(v)?,
                    "strength" => strength = v.parse().map_err(|_| format!("bad number '{v}'"))?,
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let l = links::MeshLock { name: name.to_string(), host: host.ok_or("mesh-lock add needs host=")?, follower: fol.ok_or("mesh-lock add needs follower=")?, vertices: verts, strength };
            let q = links::add_mesh_lock(&p, l)?;
            compile(q.clone(), path)?;
            write_json(&q, out)?;
            println!("added mesh lock '{name}'; wrote {out}");
            Ok(())
        }
        ["glue", "list", path] => {
            let p = load_project(path)?;
            for g in &p.glues {
                let pr: Vec<String> = g.pairs.iter().map(|x| format!("{}:{}#{}->{}:{}#{}", x.a.layer, x.a.path, x.a.index, x.b.layer, x.b.path, x.b.index)).collect();
                println!("{}: {:?} strength {} {}", g.name, g.mode, g.strength, pr.join(" "));
            }
            println!("{} glue(s)", p.glues.len());
            Ok(())
        }
        ["glue", "remove", path, out, name] => {
            let q = links::remove_glue(&load_project(path)?, name)?;
            write_json(&q, out)?;
            println!("removed glue '{name}'; wrote {out}");
            Ok(())
        }
        ["glue", "add", path, out, name, rest @ ..] => {
            let p = load_project(path)?;
            let (mut a, mut b, mut pairs, mut auto, mut mode, mut strength) = (None, None, None, None, links::GlueMode::default(), 1.0f32);
            for kv in rest {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("option '{kv}' must be key=value"))?;
                match k {
                    "a" => a = Some(links::parse_geom(v)?),
                    "b" => b = Some(links::parse_geom(v)?),
                    "pairs" => pairs = Some(links::parse_pairs(v)?),
                    "auto" => auto = Some(v.parse::<f32>().map_err(|_| format!("bad number '{v}'"))?),
                    "mode" => mode = links::GlueMode::parse(v).ok_or_else(|| format!("unknown mode '{v}' (rigid, snap, offset)"))?,
                    "strength" => strength = v.parse().map_err(|_| format!("bad number '{v}'"))?,
                    other => return Err(format!("unknown option '{other}'")),
                }
            }
            let (a, b) = (a.ok_or("glue add needs a=")?, b.ok_or("glue add needs b=")?);
            let list = match (pairs, auto) {
                (Some(_), Some(_)) => return Err("give pairs= or auto=, not both".into()),
                (Some(pr), None) => pr
                    .into_iter()
                    .map(|(i, j)| links::GluePair {
                        a: links::VertexRef { layer: a.layer.clone(), path: a.path, index: i },
                        b: links::VertexRef { layer: b.layer.clone(), path: b.path, index: j },
                    })
                    .collect(),
                (None, Some(r)) => links::auto_pairs(&p, &a, &b, r)?,
                (None, None) => return Err("glue add needs pairs=i:j;... or auto=RADIUS".into()),
            };
            let n = list.len();
            let q = links::add_glue(&p, links::Glue { name: name.to_string(), mode, pairs: list, strength })?;
            compile(q.clone(), path)?;
            write_json(&q, out)?;
            println!("added glue '{name}' ({n} pair(s)); wrote {out}");
            Ok(())
        }
        ["mocap-info", file] => {
            let b = armatura_core::mocap::parse_bvh(&fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?).map_err(|e| format!("{file}: {e}"))?;
            print!("{}", b.summary());
            Ok(())
        }
        ["mocap-match", path, file] => {
            let p = load_project(path)?;
            let b = armatura_core::mocap::parse_bvh(&fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?).map_err(|e| format!("{file}: {e}"))?;
            let map = mocap_map(&a)?;
            let m = armatura_core::mocap::match_bones(&b, &p, map.as_ref())?;
            print!("{}", armatura_core::mocap::format_table(&m));
            Ok(())
        }
        ["mocap-retarget", path, file, out] => {
            use armatura_core::mocap as mc;
            let mut p = load_project(path)?;
            let b = mc::parse_bvh(&fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?).map_err(|e| format!("{file}: {e}"))?;
            let map = mocap_map(&a)?;
            let m = mc::match_bones(&b, &p, map.as_ref())?;
            let idx = |key: &str| -> Result<Option<usize>, String> {
                a.opts.get(key).map(|v| v.parse::<usize>().map_err(|_| format!("{key} needs a frame number, got '{v}'"))).transpose()
            };
            let mut o = mc::RetargetOptions::default();
            if let Some(v) = a.opts.get("--view") { o.view = mc::View::parse(v)?; }
            o.scale = match a.opts.get("--scale").map(String::as_str) {
                None | Some("auto") => mc::Scale::Auto,
                Some(v) => mc::Scale::Fixed(v.parse().map_err(|_| format!("--scale needs 'auto' or a number, got '{v}'"))?),
            };
            if a.opts.contains_key("--fps") { o.fps = Some(num(&a, "--fps", 0.0)? as f64); }
            o.smooth = num(&a, "--smooth", 0.0)? as usize;
            o.start = idx("--start")?;
            o.end = idx("--end")?;
            o.rest_frame = idx("--rest-frame")?;
            o.min_confidence = num(&a, "--min-conf", o.min_confidence)?;
            o.weight = num(&a, "--weight", 1.0)?;
            o.mirror = a.flags.iter().any(|f| f == "--mirror");
            o.in_place = a.flags.iter().any(|f| f == "--in-place");
            o.root_motion = !a.flags.iter().any(|f| f == "--no-root");
            if let Some(bone) = a.opts.get("--footlock") {
                o.foot_lock = Some((bone.clone(), num(&a, "--ground", 0.0)?));
            }
            let r = mc::retarget(&p, &b, &m, &o)?;
            print!("{}", mc::format_table(&m));
            // the library's own "project length extended" note is replaced by the fuller message printed after the summary
            for n in r.notes.iter().filter(|n| !n.starts_with("project length extended")) { println!("note: {n}"); }
            let old_frames = p.frames;
            let keep_length = a.flags.iter().any(|f| f == "--keep-length");
            let mut cut_keys = 0usize;
            if keep_length && r.project_frames > old_frames {
                // bake the part of the clip that fits: the last frame of the project gets an interpolated key, later keys go
                let mut r2 = r.clone();
                let last = old_frames.saturating_sub(1) as f32;
                for c in &mut r2.channels {
                    if let Source::Keys { track } = &mut c.source {
                        let end = track.eval(last);
                        let before = track.keys.len();
                        track.keys.retain(|k| k.frame <= last);
                        cut_keys += before - track.keys.len();
                        if let Some(v) = end {
                            if track.keys.last().is_none_or(|k| k.frame < last) {
                                track.set_key(last, v, armatura_core::anim::Ease::Linear);
                            }
                        }
                    }
                }
                r2.project_frames = old_frames;
                mc::bake(&mut p, &r2);
            } else {
                mc::bake(&mut p, &r);
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("{} bones, {} frames at {} fps, root scale {:.4}, +{} channels; wrote {out}", r.bones_used, r.out_frames, o.fps.unwrap_or(p.fps as f64), r.scale, r.channels.len());
            if p.frames > old_frames {
                println!("note: the project length was extended from {old_frames} to {} frames to hold the {}-frame clip (every other channel keeps its keys; pass --keep-length to cut the clip at {old_frames} frames instead)", p.frames, r.out_frames);
            } else if cut_keys > 0 {
                println!("note: --keep-length: the clip is {} frames, the project {old_frames}; {cut_keys} key(s) after frame {} were not baked", r.out_frames, old_frames.saturating_sub(1));
            }
            Ok(())
        }
        ["machine", "add", path, out, file] => {
            let mut p = load_project(path)?;
            let m: armatura_core::machine::Machine = read_json(file)?;
            let name = m.name.clone();
            armatura_core::machine::add(&mut p, m)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("added machine '{name}' ({} machines); wrote {out}", p.machines.len());
            Ok(())
        }
        ["machine", "list", path] => {
            let p = load_project(path)?;
            for m in &p.machines {
                let ins: Vec<String> = m.inputs.iter().map(|i| format!("{}:{:?}", i.name, i.kind).to_lowercase()).collect();
                println!("{}  inputs [{}]", m.name, ins.join(", "));
                for l in &m.layers {
                    println!("  layer {} (entry {}, mask [{}]): {} states, {} transitions", l.name, l.entry, l.mask.join(","), l.states.len(), l.transitions.len());
                }
            }
            if p.machines.is_empty() { println!("no machines"); }
            Ok(())
        }
        ["machine", "remove", path, out, name] => {
            let mut p = load_project(path)?;
            armatura_core::machine::remove(&mut p, name)?;
            write_json(&p, out)?;
            println!("removed machine '{name}'");
            Ok(())
        }
        ["machine", "validate", path] => {
            let p = load_project(path)?;
            let mut errs = armatura_core::machine::validate(&p);
            if errs.is_empty() {
                errs.extend(armatura_core::machine::trial_errors(&p, &asset_dir(path)));
            }
            if errs.is_empty() {
                println!("{} machines, {} bindings: ok", p.machines.len(), p.bindings.len());
                Ok(())
            } else {
                Err(format!("{} problem(s):\n  - {}", errs.len(), errs.join("\n  - ")))
            }
        }
        ["machine-sim", path, script, outp] => {
            let p = load_project(path)?;
            let sc: armatura_core::machine::Script = read_json(script)?;
            let fps = num(&a, "--fps", p.fps)?;
            let as_dir = !outp.to_ascii_lowercase().ends_with(".json");
            let n_svg = match a.opts.get("--svg-frames") {
                Some(v) => v.parse::<usize>().map_err(|_| format!("--svg-frames needs a whole number, got '{v}'"))?,
                None => if as_dir { 12 } else { 0 },
            };
            if as_dir { fs::create_dir_all(outp).map_err(|e| format!("cannot create {outp}: {e}"))?; }
            // total ticks are needed to pick evenly spaced frames: run once without rendering
            let dir = asset_dir(path);
            let total = armatura_core::machine::simulate(&p, &sc, fps, &dir, |_, _| Ok(()))?.ticks.len();
            let pick: Vec<usize> = if n_svg == 0 { vec![] } else if n_svg >= total { (0..total).collect() } else { (0..n_svg).map(|i| if n_svg == 1 { 0 } else { i * (total - 1) / (n_svg - 1) }).collect() };
            let mut written = 0;
            let log = armatura_core::machine::simulate(&p, &sc, fps, &dir, |tick, rt| {
                if pick.contains(&tick) {
                    let eng = rt.engine()?;
                    let svg = render_frame(&eng, rt.base_frame(), &RenderOptions { show_bones: false });
                    let f = Path::new(outp).join(format!("frame_{tick:05}.svg"));
                    fwrite(&f, svg)?;
                    written += 1;
                }
                Ok(())
            })?;
            let json_path = if as_dir { Path::new(outp).join("sim.json").to_string_lossy().to_string() } else { outp.to_string() };
            fwrite(&json_path, serde_json::to_string_pretty(&log).map_err(|e| e.to_string())?)?;
            for e in &log.events {
                println!("{:8.3}s  {:8} {:?} {}", e.time, e.layer, e.kind, e.state);
            }
            println!("{} ticks at {fps} fps, {} events, {written} svg frames; wrote {json_path}", log.ticks.len(), log.events.len());
            Ok(())
        }
        ["bind", "add", path, out, name, vpath, target] => {
            use armatura_core::binding::{Binding, Remap, Target};
            let mut p = load_project(path)?;
            let tgt = if let Some(rest) = target.strip_prefix('@') {
                let (m, i) = rest.split_once(':').ok_or("machine target must be @machine:input")?;
                Target::Input { machine: m.to_string(), input: i.to_string() }
            } else {
                let (t, pr) = target.rsplit_once(':').ok_or("property target must be target:prop (e.g. arm:bone_angle) or @machine:input")?;
                let prop = armatura_core::project::Prop::parse(pr).ok_or_else(|| format!("unknown property '{pr}'"))?;
                Target::Prop { target: t.to_string(), prop }
            };
            let mut b = Binding::new(name, vpath, tgt);
            b.expr = a.opts.get("--expr").cloned();
            b.smooth = num(&a, "--smooth", 0.0)?;
            b.weight = num(&a, "--weight", 1.0)?;
            if let Some(m) = a.opts.get("--mode") {
                b.mode = serde_json::from_value(serde_json::Value::String(m.clone())).map_err(|_| format!("unknown mode '{m}' (replace, add, multiply)"))?;
            }
            if let Some(r) = a.opts.get("--remap") {
                let v: Vec<f32> = r.split(',').map(|x| x.trim().parse::<f32>().map_err(|_| format!("bad number '{x}' in --remap"))).collect::<Result<_, _>>()?;
                if v.len() != 4 { return Err("--remap needs in_min,in_max,out_min,out_max".into()); }
                b.remap = Some(Remap { in_min: v[0], in_max: v[1], out_min: v[2], out_max: v[3], clamp: !a.flags.iter().any(|f| f == "--no-clamp"), curve: armatura_core::Ease::Linear });
            }
            armatura_core::machine::add_binding(&mut p, b)?;
            write_json(&p, out)?;
            println!("added binding '{name}' ({} bindings); wrote {out}", p.bindings.len());
            Ok(())
        }
        ["bind", "list", path] => {
            let p = load_project(path)?;
            for b in &p.bindings {
                let tgt = match &b.target {
                    armatura_core::binding::Target::Prop { target, prop } => format!("{target}:{prop:?}"),
                    armatura_core::binding::Target::Input { machine, input } => format!("@{machine}:{input}"),
                };
                let mut extra = String::new();
                if let Some(e) = &b.expr { extra += &format!(" expr \"{e}\""); }
                if let Some(r) = &b.remap { extra += &format!(" remap {}..{} -> {}..{}{}", r.in_min, r.in_max, r.out_min, r.out_max, if r.clamp { " clamp" } else { "" }); }
                if b.smooth > 0.0 { extra += &format!(" smooth {}s", b.smooth); }
                println!("{}  {} -> {tgt}{extra}", b.name, b.path);
            }
            if p.bindings.is_empty() { println!("no bindings"); }
            Ok(())
        }
        ["bind", "remove", path, out, name] => {
            let mut p = load_project(path)?;
            armatura_core::machine::remove_binding(&mut p, name)?;
            write_json(&p, out)?;
            println!("removed binding '{name}'");
            Ok(())
        }
        ["bind-eval", path, values] => {
            let p = load_project(path)?;
            let vals: std::collections::BTreeMap<String, f64> = read_json(values)?;
            let list: Vec<(String, f64)> = vals.into_iter().collect();
            let outv = armatura_core::binding::eval_once(&p.bindings, &list)?;
            let mut obj = serde_json::Map::new();
            for (n, v) in &outv {
                println!("{n} = {v}");
                obj.insert(n.clone(), serde_json::json!(v));
            }
            if let Some(o) = a.opts.get("--data") {
                fwrite(o, serde_json::to_string_pretty(&obj).map_err(|e| e.to_string())?)?;
            }
            let unbound = p.bindings.len() - outv.len();
            if unbound > 0 { println!("{unbound} binding(s) have no value in {values}"); }
            Ok(())
        }
        ["container", "add", path, out, file] => {
            let mut p = load_project(path)?;
            let c: armatura_core::containers::Container = read_json(file)?;
            let id = c.id.clone();
            armatura_core::containers::add_container(&mut p, c)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("added container '{id}' ({} containers); wrote {out}", p.containers.len());
            Ok(())
        }
        ["container", "list", path] => {
            let p = load_project(path)?;
            for c in &p.containers {
                println!("{}: {:?}, {} members, gap {}, justify {:?}, align {:?}, wrap {:?}, rect {}", c.id, c.axis, c.members.len(), c.gap, c.justify, c.align, c.wrap, serde_json::to_string(&c.rect).unwrap_or_default());
            }
            if p.containers.is_empty() { println!("no containers"); }
            Ok(())
        }
        ["container", "remove", path, out, id] => {
            let mut p = load_project(path)?;
            armatura_core::containers::remove_container(&mut p, id)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("removed container '{id}'");
            Ok(())
        }
        ["container", "solve", path] => {
            let mut p = load_project(path)?;
            if let Some(sz) = a.opts.get("--size") {
                let (w, h) = armatura_core::containers::parse_size(sz)?;
                p = armatura_core::containers::responsive(&p, w, h)?;
            }
            let sol = armatura_core::containers::solve_project(&p)?;
            println!("canvas {}x{}", sol.canvas.0, sol.canvas.1);
            for (id, r) in &sol.rects { println!("container {id}: x {:.2} y {:.2} w {:.2} h {:.2}", r[0], r[1], r[2], r[3]); }
            for l in &sol.layers { println!("layer {}: dx {:.3} dy {:.3} scale {:.4} x {:.4}", l.layer, l.dx, l.dy, l.kx, l.ky); }
            for s in &sol.scrolls { println!("scroll {}: viewport {:?} content {:?} max {:?} offset {:?}", s.id, s.viewport, s.content, s.max, s.offset); }
            if let Some(d) = a.opts.get("--data") {
                let v = serde_json::json!({
                    "canvas": [sol.canvas.0, sol.canvas.1],
                    "containers": sol.rects.iter().map(|(i, r)| serde_json::json!({"id": i, "rect": r})).collect::<Vec<_>>(),
                    "layers": sol.layers.iter().map(|l| serde_json::json!({"layer": l.layer, "dx": l.dx, "dy": l.dy, "kx": l.kx, "ky": l.ky})).collect::<Vec<_>>(),
                    "scrolls": sol.scrolls.iter().map(|s| serde_json::json!({"id": s.id, "viewport": s.viewport, "content": s.content, "max": s.max, "offset": s.offset})).collect::<Vec<_>>(),
                });
                fwrite(d, serde_json::to_string_pretty(&v).unwrap_or_default())?;
            }
            Ok(())
        }
        ["scroll", "add", path, out, file] => {
            let mut p = load_project(path)?;
            let s: armatura_core::containers::Scroll = read_json(file)?;
            let id = s.id.clone();
            armatura_core::containers::add_scroll(&mut p, s)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("added scroll '{id}' ({} scrolls); wrote {out}", p.scrolls.len());
            Ok(())
        }
        ["scroll", "list", path] => {
            let p = load_project(path)?;
            for s in &p.scrolls {
                println!("{}: content {}, axis {:?}, offset {:?}, elastic {}, snap x {:?} y {:?}, clip {}", s.id, s.content, s.axis, s.offset, s.elastic.is_some(), s.snap_x, s.snap_y, s.clip_layer.as_deref().unwrap_or("-"));
            }
            if p.scrolls.is_empty() { println!("no scroll regions"); }
            Ok(())
        }
        ["scroll", "remove", path, out, id] => {
            let mut p = load_project(path)?;
            armatura_core::containers::remove_scroll(&mut p, id)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("removed scroll '{id}'");
            Ok(())
        }
        ["scroll", "eval", path, id] => {
            use armatura_core::containers as ct;
            let mut p = load_project(path)?;
            if let Some(sz) = a.opts.get("--size") {
                let (w, h) = ct::parse_size(sz)?;
                p = ct::responsive(&p, w, h)?;
            }
            let pair = |key: &str| -> Result<Option<[f32; 2]>, String> {
                match a.opts.get(key) {
                    None => Ok(None),
                    Some(v) => {
                        let (x, y) = v.split_once(',').ok_or_else(|| format!("{key} needs X,Y"))?;
                        Ok(Some([x.trim().parse().map_err(|_| format!("bad number in {key}"))?, y.trim().parse().map_err(|_| format!("bad number in {key}"))?]))
                    }
                }
            };
            if let Some(o) = pair("--offset")? { ct::set_offset(&mut p, id, o)?; }
            let sol = ct::solve_project(&p)?;
            let info = sol.scrolls.iter().find(|s| &s.id == id).ok_or_else(|| format!("no scroll '{id}'"))?;
            println!("viewport {:?} content {:?} max {:?}", info.viewport, info.content, info.max);
            println!("requested offset {:?} -> applied {:?}", p.scrolls.iter().find(|s| &s.id == id).map(|s| s.offset).unwrap_or([0.0; 2]), info.offset);
            if let Some(v) = pair("--velocity")? {
                let sc = p.scrolls.iter().find(|s| &s.id == id).ok_or_else(|| format!("no scroll '{id}'"))?;
                let mut rt = ct::ScrollRuntime::new(sc, info.max);
                rt.drag_to(sc.offset);
                rt.release(v);
                let total = num(&a, "--time", 2.0)?;
                let steps = (total * 60.0).round().max(1.0) as usize;
                for i in 1..=steps {
                    rt.step(1.0 / 60.0);
                    if i % 6 == 0 || i == steps || rt.is_settled() { println!("t {:.3}s offset [{:.3}, {:.3}]", i as f32 / 60.0, rt.pos[0], rt.pos[1]); }
                    if rt.is_settled() { println!("settled"); break; }
                }
            }
            Ok(())
        }
        ["layout-preview", path, out] => {
            use armatura_core::containers as ct;
            let mut p = load_project(path)?;
            let sz = a.opts.get("--size").ok_or("layout-preview needs --size WxH")?;
            let (w, h) = ct::parse_size(sz)?;
            p = ct::responsive(&p, w, h)?;
            if let Some(spec) = a.opts.get("--scroll") {
                for part in spec.split(';') {
                    let (id, v) = part.split_once('=').ok_or("--scroll needs id=X,Y")?;
                    let (x, y) = v.split_once(',').ok_or("--scroll needs id=X,Y")?;
                    ct::set_offset(&mut p, id, [x.trim().parse().map_err(|_| "bad scroll x")?, y.trim().parse().map_err(|_| "bad scroll y")?])?;
                }
            }
            let sol = ct::solve_project(&p)?;
            let e = compile(p, path)?;
            let frame = num(&a, "--frame", 0.0)?;
            let svg = ct::with_overlay(&render_frame(&e, frame, &RenderOptions { show_bones: false }), &sol);
            fwrite(out, svg)?;
            println!("wrote {out} ({w}x{h}, {} containers, {} layers placed)", sol.rects.len(), sol.layers.len());
            Ok(())
        }
        ["pick", path] => {
            let e = load(path)?;
            let frame = num(&a, "--frame", 0.0)?;
            let p = Vec2::new(num(&a, "--x", f32::NAN)?, num(&a, "--y", f32::NAN)?);
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err("pick needs --x and --y".into());
            }
            let o = pick_opts(&a)?;
            let fr = e.evaluate(frame);
            let hits = armatura_core::pick::pick_all(&e, &fr, p, &o);
            if a.flags.iter().any(|f| f == "--json") {
                println!("{}", serde_json::json!({"hit": hits.first(), "hits": hits}));
            } else if hits.is_empty() {
                println!("nothing at ({}, {}) on frame {frame}", p.x, p.y);
            } else {
                for (i, h) in hits.iter().enumerate() {
                    println!("{}{} (distance {:.2})", if i == 0 { "hit: " } else { "  under: " }, describe_target(&h.target), h.distance);
                }
            }
            Ok(())
        }
        ["select-box", path] => {
            use armatura_core::pick::{MarqueeMode, Region};
            let e = load(path)?;
            let frame = num(&a, "--frame", 0.0)?;
            let o = pick_opts(&a)?;
            let mode = match a.opts.get("--mode") {
                None => MarqueeMode::Intersect,
                Some(m) => MarqueeMode::parse(m).ok_or_else(|| format!("--mode must be bbox, contain or intersect, got '{m}'"))?,
            };
            let nums = |s: &str, sep: char| -> Result<Vec<f32>, String> { s.split(sep).map(|t| t.trim().parse::<f32>().map_err(|_| format!("bad number '{t}'"))).collect() };
            let region = if let Some(r) = a.opts.get("--rect") {
                let n = nums(r, ',')?;
                if n.len() != 4 {
                    return Err("--rect needs x0,y0,x1,y1".into());
                }
                Region::rect(Vec2::new(n[0], n[1]), Vec2::new(n[2], n[3]))
            } else if let Some(l) = a.opts.get("--lasso") {
                let mut pts = Vec::new();
                for pair in l.split(';') {
                    let n = nums(pair, ',')?;
                    if n.len() != 2 {
                        return Err("--lasso needs x,y;x,y;x,y".into());
                    }
                    pts.push(Vec2::new(n[0], n[1]));
                }
                if pts.len() < 3 {
                    return Err("--lasso needs at least 3 points".into());
                }
                Region::Lasso(pts)
            } else {
                return Err("select-box needs --rect or --lasso".into());
            };
            let t = armatura_core::pick::marquee(&e, &e.evaluate(frame), &region, mode, &o);
            if a.flags.iter().any(|f| f == "--json") {
                println!("{}", serde_json::json!({"targets": t}));
            } else if t.is_empty() {
                println!("nothing selected");
            } else {
                for x in &t {
                    println!("{}", describe_target(x));
                }
            }
            Ok(())
        }
        ["timeline", path] => {
            let p = load_project(path)?;
            let m = armatura_core::timeline::build(&p);
            if a.flags.iter().any(|f| f == "--json") {
                println!("{}", serde_json::to_string(&m).map_err(|e| e.to_string())?);
            } else {
                print!("{}", timeline_text(&m));
            }
            Ok(())
        }
        ["keyedit", path, out, sub] => keyedit_cmd(&a, path, out, sub),
        #[cfg(feature = "gpu")]
        ["render-gpu", path, out] => render_gpu_cmd(&a, path, out),
        #[cfg(not(feature = "gpu"))]
        ["render-gpu", ..] => Err("render-gpu needs a build with the gpu feature: cargo build -p armatura-cli --features gpu".to_string()),
        ["collab-sim", path] => collab_sim(&a, path),
        ["vcs", rest @ ..] => vcs_cmd::run(rest, &a.opts, &a.flags),
        ["fuzz", path] => fuzz_cmd::fuzz(path, &a.opts, &a.flags),
        ["fuzz-grid", path, out] => fuzz_cmd::grid(path, out, &a.opts, &a.flags),
        ["ci", ..] => ci_cmd::run(raw),
        ["export-fit", path, out] => fit_cmd::export_fit(path, out, &a.opts, &a.flags),
        ["size-report", path] => fit_cmd::size_report(path, &a.opts, &a.flags),
        ["lod", path] => lod_cmd::lod(path, a.scale, &a.opts, &a.flags),
        ["lod-pick", path] => lod_cmd::pick(path, &a.opts),
        ["face-train"] => face_cmd::train(&a.opts),
        ["face-eval"] => face_cmd::eval(&a.opts, &a.flags),
        ["face-detect", image] => face_cmd::detect(image, &a.opts, &a.flags),
        ["face-track", input, out] => face_cmd::track(input, out, &a.opts),
        ["face-synth", dir] => face_cmd::synth(dir, &a.opts),
        ["collab-token", name] => {
            let secret = collab_srv::secret_from(a.opts.get("--secret-file").map(String::as_str))?;
            let ttl = num(&a, "--ttl", 28800.0)?;
            if !matches!(ttl.partial_cmp(&1.0), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)) || ttl.fract() != 0.0 {
                return Err("--ttl needs a whole number of seconds (1 to 2592000)".into());
            }
            let now = armatura_core::clock::unix_seconds();
            let token = armatura_core::collabauth::mint_token(&secret, name, now, ttl as u64)?;
            println!("{token}");
            eprintln!("token for '{name}', valid until Unix time {} (in {} s); give it to that person only; send it as the X-Collab-Token header or enter it on the login page", now + ttl as u64, ttl as u64);
            Ok(())
        }
        ["color-space", path, out, space] => {
            let mut p = load_project(path)?;
            p.color_space = armatura_core::project::ColorSpace::parse(space)?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out} (color_space: {})", if p.color_space.is_default() { "srgb" } else { "linear" });
            Ok(())
        }
        ["reparent", path, out, bone, parent] => {
            let mut p = load_project(path)?;
            rigedit::set_parent(&mut p, bone, if *parent == "none" { None } else { Some(parent) })?;
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["comp", "list", path] | ["comp", "list", path, _] => {
            // the 4th word is accepted for old scripts (`comp list p.json p.json`); nothing is written
            for c in &load_project(path)?.comps {
                println!("{} ({} layers)", c.name, c.layers.len());
            }
            Ok(())
        }
        ["comp", mode @ ("capture" | "apply"), path, out, name] => {
            let mut p = load_project(path)?;
            if *mode == "capture" {
                comps::capture(&mut p, name)?;
            } else {
                for m in comps::apply(&mut p, name)? {
                    println!("warning: layer '{m}' no longer exists");
                }
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["comp", mode @ ("capture" | "apply"), ..] => Err(format!("comp {mode} needs <project.json> <out.json> <name>")),
        ["comp", other, ..] => Err(format!("unknown comp action '{other}' (capture, apply, list)")),
        ["event", path, out, name, frame] => {
            let mut p = load_project(path)?;
            let data = match a.opts.get("--data") {
                Some(d) => serde_json::from_str(d).map_err(|e| format!("--data is not valid JSON: {e}"))?,
                None => serde_json::Value::Null,
            };
            p.events.push(comps::Event { name: name.to_string(), frame: frame.parse().map_err(|_| format!("bad frame '{frame}'"))?, data });
            p.events.sort_by(|x, y| x.frame.total_cmp(&y.frame));
            write_json(&p, out)?;
            println!("wrote {out} ({} events)", p.events.len());
            Ok(())
        }
        ["snapshot", mode @ ("save" | "list"), path, dir] => {
            let s = snapshots::Snapshots::new(dir, num(&a, "--keep", 20.0)? as usize);
            if *mode == "save" {
                match s.save(&load_project(path)?)? {
                    Some(r) => println!("saved revision {} ({})", r.number, r.hash),
                    None => println!("unchanged since the last revision"),
                }
            } else {
                for r in s.list() {
                    println!("{:>6}  {}  {}", r.number, r.hash, r.path.display());
                }
            }
            Ok(())
        }
        ["snapshot", mode @ ("restore" | "recover"), _path, dir, out] => {
            let s = snapshots::Snapshots::new(dir, 20);
            let p = if *mode == "recover" {
                s.recover().ok_or("no readable revision found")?
            } else {
                s.load(a.opts.get("--number").map(|n| n.parse::<u64>().map_err(|_| "bad --number".to_string())).transpose()?)?
            };
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["snapshot", mode @ ("save" | "list"), ..] => Err(format!("snapshot {mode} needs <project.json> <dir> and nothing else")),
        ["snapshot", mode @ ("restore" | "recover"), ..] => Err(format!("snapshot {mode} needs <project.json> <dir> <out.json>")),
        ["snapshot", other, ..] => Err(format!("unknown snapshot action '{other}' (save, list, restore, recover)")),
        ["explain", path, target, frame] => {
            let e = load(path)?;
            let x = explain::explain(&e, target, frame.parse().map_err(|_| format!("bad frame '{frame}'"))?)?;
            for l in &x.lines {
                println!("- {l}");
            }
            Ok(())
        }
        ["spring-fit"] => {
            let samples: Vec<f32> = a
                .opts
                .get("--samples")
                .ok_or("--samples a,b,c,… is required")?
                .split(',')
                .map(|s| s.trim().parse().map_err(|_| format!("bad sample '{s}'")))
                .collect::<Result<_, _>>()?;
            let fit = springfit::fit_spring(num(&a, "--start", 0.0)?, &samples, num(&a, "--fps", 24.0)?)?;
            println!("frequency {:.2} Hz, damping {:.2} (rms error {:.3} of the step size)", fit.frequency, fit.damping, fit.rms);
            println!("add to the project: {{\"bone\": \"<bone>\", \"frequency\": {:.2}, \"damping\": {:.2}}}", fit.frequency, fit.damping);
            Ok(())
        }
        ["compare", a_path, b_path, frame, out] => {
            let (ea, eb) = (load(a_path)?, load(b_path)?);
            let (w, h, px) = inspect::compare_frame(&ea, &eb, frame.parse().map_err(|_| format!("bad frame '{frame}'"))?, a.scale, num(&a, "--gain", 4.0)?)?;
            fwrite(out, open_formats::encode_rgba_png(w, h, &px)?).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        ["mcp", path] => {
            let p = load_project(path)?;
            let ed = Editor::with_asset_dir(p, asset_dir(path)).map_err(|errs| format!("{path} is not a valid project:\n  - {}", errs.join("\n  - ")))?;
            let mut ag = agent::Agent::new(ed, Some(std::path::PathBuf::from(path)));
            let stdin = std::io::stdin();
            ag.serve(stdin.lock(), std::io::stdout().lock()).map_err(|e| e.to_string())
        }
        ["fx", path, frame, chain_path, out] => {
            let e = load(path)?;
            let frame: f32 = frame.parse().map_err(|_| format!("bad frame number '{frame}'"))?;
            let frame = check_frame_in_range(frame, e.project.frames, a.flags.iter().any(|f| f == "--clamp"))?;
            let mut chain: Vec<postfx::Fx> = read_json(chain_path)?;
            let chain_dir = std::path::Path::new(chain_path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
            postfx::resolve_normal_map_paths(&mut chain, &chain_dir)?;
            let (w, h, px) = if a.flags.iter().any(|f| f == "--layer-normals") {
                postfx::render_with_layer_normals(&e, frame, a.scale, &chain, &asset_dir(path))?
            } else {
                let (w, h, mut px) = armatura_core::raster::render_rgba(&e, frame, &RasterOptions { scale: a.scale, show_bones: false, transparent: false });
                postfx::apply(&chain, w as usize, h as usize, &mut px)?;
                (w, h, px)
            };
            fwrite(out, open_formats::encode_rgba_png(w, h, &px)?).map_err(|e| e.to_string())?;
            println!("wrote {out} ({} effect(s))", chain.len());
            Ok(())
        }
        ["layer-normal", path, out, layer, map] => {
            let mut p = load_project(path)?;
            let l = p.find_layer_mut(layer).ok_or_else(|| format!("no layer '{layer}'"))?;
            if *map == "none" {
                l.normal_map = None;
            } else {
                let bytes = fs::read(map).map_err(|e| format!("cannot read {map}: {e}"))?;
                let fit = match a.opts.get("--fit").map(String::as_str).unwrap_or("stretch") {
                    "stretch" => postfx::NormalFit::Stretch,
                    "tile" => postfx::NormalFit::Tile,
                    "exact" => postfx::NormalFit::Exact,
                    o => return Err(format!("unknown --fit '{o}' (stretch, tile, exact)")),
                };
                let nm = postfx::NormalMap { data: Some(armatura_core::base64::encode(&bytes)), path: None, fit, strength: num(&a, "--strength", 1.0)?, green_down: a.flags.iter().any(|f| f == "--green-down") };
                l.normal_map = Some(nm);
            }
            compile(p.clone(), path)?;
            write_json(&p, out)?;
            println!("wrote {out}");
            Ok(())
        }
        ["onion", path, out] => {
            let e = load(path)?;
            let spec = onion_spec_from(&a)?;
            let frame = num(&a, "--frame", 0.0)?;
            if frame < 0.0 || frame.fract() != 0.0 {
                return Err("--frame needs a whole frame number".into());
            }
            let svg = if a.flags.iter().any(|f| f == "--ghosts-only") { overlays::onion_svg(&e, frame as u32, &spec, &[])? } else { overlays::onion_composite_svg(&e, frame as u32, &spec, &[])? };
            let n = overlays::ghosts_for(&e, frame as u32, &spec, &[])?.len();
            fwrite(out, svg)?;
            println!("wrote {out} ({n} ghost(s) around frame {frame})");
            Ok(())
        }
        ["trail", path] | ["trail", path, _] if a.opts.contains_key("--target") => {
            let e = load(path)?;
            let t = overlays::TrailTarget::parse(&a.opts["--target"])?;
            let from = num(&a, "--from", 0.0)?;
            let to = num(&a, "--to", e.project.frames.max(1) as f32 - 1.0)?;
            if from < 0.0 || to < 0.0 || from.fract() != 0.0 || to.fract() != 0.0 {
                return Err("--from/--to need whole frame numbers".into());
            }
            let sub = num(&a, "--smooth", 0.0)?;
            if sub < 0.0 || sub.fract() != 0.0 {
                return Err("--smooth needs a whole number of subdivisions (0 = off)".into());
            }
            let tr = overlays::trails(&e, &[t], from as u32, to as u32, sub as u32)?;
            if let Some(out) = a.pos.get(2) {
                fwrite(out, overlays::trails_svg(&e, &tr, None))?;
                if !a.flags.iter().any(|f| f == "--json") {
                    println!("wrote {out} ({} ticks, {} key marker(s))", tr[0].points.len(), tr[0].keys.len());
                }
            }
            if a.flags.iter().any(|f| f == "--json") {
                println!("{}", serde_json::to_string_pretty(&overlays::trail_json(&tr[0])).unwrap_or_default());
            } else if a.pos.len() < 3 {
                println!("{}", tr[0].target);
                for (f, p) in tr[0].frames.iter().zip(&tr[0].points) {
                    let k = if tr[0].keys.iter().any(|k| (k - *f as f32).abs() <= 0.5) { " key" } else { "" };
                    println!("{f:>5}  {:>9.3} {:>9.3}{k}", p.x, p.y);
                }
            }
            Ok(())
        }
        ["proxy-bench", path] => {
            let e = load(path)?;
            let n = (num(&a, "--frames", 24.0)? as u32).clamp(1, e.project.frames.max(1));
            let time = |level: u8| -> Result<f64, String> {
                let t0 = std::time::Instant::now();
                for f in 0..n {
                    armatura_core::scrub::render_proxy_rgba(&e, f as f32, level)?;
                }
                Ok(t0.elapsed().as_secs_f64() * 1000.0 / n as f64)
            };
            let svg_t0 = std::time::Instant::now();
            for f in 0..n {
                render_frame(&e, f as f32, &RenderOptions::default());
            }
            let svg_ms = svg_t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
            let vec_ms = |level: u8| {
                let t = std::time::Instant::now();
                for f in 0..n {
                    let _ = armatura_core::scrub::proxy_svg_vector(&e, f as f32, level, false);
                }
                t.elapsed().as_secs_f64() * 1000.0 / n as f64
            };
            let (v0, v2, v4) = (vec_ms(0), vec_ms(2), vec_ms(4));
            let t0 = std::time::Instant::now();
            for f in 0..n {
                e.evaluate(f as f32);
            }
            let eval_ms = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
            let full = time(0)?;
            println!("{n} frames of {} ({}x{}), ms per frame, single thread:", e.project.name, e.project.width, e.project.height);
            println!("  evaluate only       {eval_ms:8.2}");
            println!("  svg (no raster)     {svg_ms:8.2}   vector proxy: level 0 {v0:.2}, level 2 {v2:.2}, level 4 {v4:.2}");
            println!("  raster level 0      {full:8.2}   (full quality)");
            for l in 1..=armatura_core::scrub::MAX_LEVEL {
                let t = time(l)?;
                println!("  raster level {l}      {t:8.2}   ({:.1}x faster than level 0)", full / t.max(1e-9));
            }
            Ok(())
        }
        ["onion", path, frame, out] => {
            let e = load(path)?;
            let frame: u32 = frame.parse().map_err(|_| format!("bad frame number '{frame}'"))?;
            let d = overlays::OnionOptions::default();
            let o = overlays::OnionOptions {
                before: num(&a, "--before", d.before as f32)? as u32,
                after: num(&a, "--after", d.after as f32)? as u32,
                step: num(&a, "--step", d.step as f32)? as u32,
                scale: a.scale,
                ..d
            };
            let (w, h, px) = overlays::onion_skin(&e, frame, &o)?;
            fwrite(out, open_formats::encode_rgba_png(w, h, &px)?).map_err(|e| e.to_string())?;
            println!("wrote {out}");
            Ok(())
        }
        ["trail", path, bone, out] => {
            let e = load(path)?;
            let pts = overlays::motion_trail(&e, bone, true, num(&a, "--from", 0.0)? as u32, num(&a, "--to", (e.project.frames - 1) as f32)? as u32)?;
            fwrite(out, overlays::trail_svg(&e, &pts, "#ff3366")).map_err(|e| e.to_string())?;
            println!("wrote {out} ({} points)", pts.len());
            Ok(())
        }
        ["studio", path] => {
            let p = load_project(path)?;
            let ed = Editor::with_asset_dir(p, asset_dir(path))
                .map_err(|errs| format!("{path} is not a valid project:\n  - {}", errs.join("\n  - ")))?;
            for w in unknown_field_warnings(path) {
                eprintln!("warning: {w} (ignored)");
            }
            let port = a.opts.get("--port").map(String::as_str).unwrap_or("8080");
            let ui_path = a.opts.get("--ui-config").map(std::path::PathBuf::from).or_else(uiconfig::default_config_path);
            let bind = a.opts.get("--bind").map(String::as_str).unwrap_or("127.0.0.1");
            let token = a.opts.get("--collab-token").cloned();
            if let Some(t) = &token {
                collab_srv::check_token(t)?;
            }
            let secret = match a.opts.get("--collab-secret-file") {
                Some(f) => Some(collab_srv::read_secret_file(f)?),
                None => None,
            };
            let revoke: Vec<String> = a.opts.get("--collab-revoke").map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
            if !revoke.is_empty() && secret.is_none() {
                return Err("--collab-revoke needs --collab-secret-file (it revokes per-participant tokens)".into());
            }
            let rate = match a.opts.get("--collab-rate") {
                Some(v) => Some(v.parse::<f64>().ok().filter(|r| r.is_finite() && *r >= 0.0 && *r <= 100000.0).ok_or_else(|| format!("--collab-rate needs a number of requests per second (0 = off), got '{v}'"))?),
                None => None,
            };
            let security = studio::Security { token, secret, revoke, rate };
            let allow_hosts: Vec<String> = a.opts.get("--allow-host").map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
            if a.flags.iter().any(|f| f == "--collab-nosync") && !a.opts.contains_key("--collab-log") {
                return Err("--collab-nosync only changes how --collab-log is flushed; it needs --collab-log LOG (nothing was started)".into());
            }
            let collab_log = a.opts.get("--collab-log").cloned().map(|p| (p, !a.flags.iter().any(|f| f == "--collab-nosync")));
            let source_url = a.opts.get("--source-url").cloned();
            if let Some(u) = &source_url {
                studio::check_source_url(u)?;
            }
            let source_url = source_url.or_else(|| studio::default_source_url().map(String::from));
            let public_origin = a.opts.get("--public-origin").cloned();
            if let Some(o) = &public_origin {
                studio::public_origin_host(o)?;
            }
            studio::serve(ed, path, &format!("{bind}:{port}"), ui_path, security, collab_log, &allow_hosts, source_url, public_origin)
        }
        ["ui", rest @ ..] => ui_cmd(&a, rest),
        _ => Err(USAGE.to_string()),
    }
}

/// `armatura ui ...`: look at the studio command registry, shortcuts and layouts without a browser.
fn ui_cmd(a: &Args, rest: &[&str]) -> Result<(), String> {
    use uiconfig::*;
    let json = a.flags.iter().any(|f| f == "--json");
    let store = || -> UiStore {
        let p = a.opts.get("--ui-config").map(std::path::PathBuf::from).or_else(default_config_path).filter(|p| p.exists());
        UiStore::open(p)
    };
    match rest {
        ["commands"] => {
            let s = store();
            if json {
                println!("{}", s.view()["commands"]);
                return Ok(());
            }
            let eff = effective_bindings(&s.cfg.shortcuts).unwrap_or_default();
            let mut cat = "";
            for c in COMMANDS {
                if c.category != cat {
                    cat = c.category;
                    println!("{cat}");
                }
                let keys: Vec<String> = eff.iter().filter(|b| b.command == c.id).map(|b| format!("{}:{}", b.context.name(), b.chord)).collect();
                println!("  {:<28} {:<34} {}", c.id, c.title, keys.join("  "));
            }
            println!("{} commands", COMMANDS.len());
            Ok(())
        }
        ["shortcuts"] => {
            let s = store();
            let eff = effective_bindings(&s.cfg.shortcuts).map_err(|e| e.join("\n"))?;
            if json {
                let rows: Vec<serde_json::Value> = eff.iter().map(|b| serde_json::json!({"command": b.command, "context": b.context.name(), "chord": b.chord.to_string(), "user": b.user})).collect();
                println!("{}", serde_json::Value::Array(rows));
                return Ok(());
            }
            for ctx in Context::ALL {
                println!("{}", ctx.name());
                let mut rows: Vec<&Binding> = eff.iter().filter(|b| b.context == ctx).collect();
                rows.sort_by(|x, y| x.chord.cmp(&y.chord).then(x.command.cmp(&y.command)));
                for b in rows {
                    println!("  {:<22} {}{}", b.chord.to_string(), b.command, if b.user { "  (yours)" } else { "" });
                }
            }
            let cf = conflicts(&eff);
            for c in &cf {
                println!("CONFLICT {}: {} vs {}", c.chord, c.a.0, c.b.0);
            }
            println!("{} bindings, {} conflicts", eff.len(), cf.len());
            Ok(())
        }
        ["layout", "presets"] => {
            for n in PRESET_NAMES {
                let l = preset(n).ok_or("preset missing")?;
                let ok = l.validate().and_then(|_| l.fits(1100.0, 640.0).map_err(|e| vec![e]));
                println!("{:<8} {}  [{}]", n, l.summary(), if ok.is_ok() { "valid" } else { "INVALID" });
            }
            Ok(())
        }
        ["layout", "show", name] => {
            let l = preset(name).ok_or_else(|| format!("no preset '{name}' (try: {})", PRESET_NAMES.join(", ")))?;
            println!("{}", serde_json::to_string_pretty(&l).map_err(|e| e.to_string())?);
            Ok(())
        }
        ["layout", "validate", file] => {
            let t = fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?;
            let l: Layout = serde_json::from_str(&t).map_err(|e| format!("{file}: {e}"))?;
            l.validate().map_err(|e| format!("{file} is not a valid layout:\n  - {}", e.join("\n  - ")))?;
            println!("{file}: valid layout ({})", l.summary());
            Ok(())
        }
        ["search", q @ ..] if !q.is_empty() => {
            let s = store();
            let v = s.search_view(&q.join(" "), 20);
            if json {
                println!("{v}");
                return Ok(());
            }
            let hits = v["hits"].as_array().cloned().unwrap_or_default();
            for h in &hits {
                let keys: Vec<&str> = h["bindings"].as_array().map(|b| b.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                println!("{:>4}  {:<28} {:<30} {}", h["score"], h["id"].as_str().unwrap_or(""), h["title"].as_str().unwrap_or(""), keys.join("  "));
            }
            if hits.is_empty() {
                println!("no command matches '{}'", q.join(" "));
            }
            Ok(())
        }
        _ => Err("usage: armatura ui commands|shortcuts [--json] | ui layout presets|show <name>|validate <file> | ui search <query>".into()),
    }
}

fn main() -> ExitCode {
    // Windows gives the main thread a 1 MiB stack (Linux/macOS: 8 MiB); deep recursion in the evaluator
    // and the renderers overflowed it (found by the Windows CI run). Run everything on a big-stack thread.
    match std::thread::Builder::new().stack_size(256 << 20).spawn(real_main) {
        Ok(h) => h.join().unwrap_or(ExitCode::FAILURE),
        Err(_) => real_main(),
    }
}

/// True when this process is the only one attached to its console, which is what happens when the .exe is opened
/// from Explorer (a terminal's shell is a second process on the console).
#[cfg(windows)]
fn launched_by_double_click() -> bool {
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    let mut ids = [0u32; 4];
    // SAFETY: `ids` is a valid buffer of the stated length; the call only writes process ids into it.
    let n = unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) };
    n == 1
}

#[cfg(windows)]
fn wait_for_enter(msg: &str) {
    println!("{msg}");
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}

/// Double-click on Windows: instead of printing the usage and closing the window at once, create a first
/// project (never overwriting an existing one) in `Documents\ARMATURA`, open the studio in the default browser
/// and keep running until the window is closed.
#[cfg(windows)]
fn double_click_launcher() -> ExitCode {
    println!("ARMATURA {}  -  command-line tool and browser studio", env!("CARGO_PKG_VERSION"));
    println!("This window runs the studio. Close it (or press Ctrl+C) to stop. For the commands, open a terminal: armatura --help\n");
    let home = env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let dir = std::path::Path::new(&home).join("Documents").join("ARMATURA");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        wait_for_enter(&format!("cannot create {}: {e}\nPress Enter to close.", dir.display()));
        return ExitCode::FAILURE;
    }
    let project = dir.join("first-rig.json").to_string_lossy().into_owned();
    if !std::path::Path::new(&project).exists() {
        if let Err(e) = run(&["demo".to_string(), project.clone()]) {
            wait_for_enter(&format!("{e}\nPress Enter to close."));
            return ExitCode::FAILURE;
        }
    }
    println!("Project: {project} (edits are saved there)");
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = Command::new("cmd").args(["/C", "start", "", "http://127.0.0.1:8080"]).spawn();
    });
    let r = run(&["studio".to_string(), project, "--port".to_string(), "8080".to_string()]);
    if let Err(e) = r {
        wait_for_enter(&format!("{e}\n(If another program uses port 8080, close it, or run: armatura studio <project.json> --port 8090)\nPress Enter to close."));
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn real_main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    // project files may only reference files inside their own directory unless this is given
    armatura_core::assets::set_allow_external(args.iter().any(|a| a == "--allow-external-assets"));
    armatura_core::fsutil::set_follow_symlinks(args.iter().any(|a| a == "--follow-symlinks"));
    #[cfg(windows)]
    if args.is_empty() && launched_by_double_click() {
        return double_click_launcher();
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            match ci_cmd::EXIT.load(std::sync::atomic::Ordering::SeqCst) {
                0 | 1 => ExitCode::FAILURE,
                c => ExitCode::from(c),
            }
        }
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    /// C12g: `ok` may not be true when the merged project does not compile.
    #[test]
    fn collab_sim_ok_requires_a_compiling_merge() {
        assert!(collab_ok(true, true));
        assert!(!collab_ok(true, false));
        assert!(!collab_ok(false, true));
    }

    /// H3 (wave 4): the same word is a flag in one command and a value option in another.
    #[test]
    fn option_table_is_per_command() {
        assert!(!takes_value("ci", "--prune") && !takes_value("export-oca", "--layers") && !takes_value("keyedit", "--free"));
        assert!(takes_value("autotrace", "--png") && takes_value("render", "--scale") && takes_value("lod", "--tolerance-px"));
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let a = parse_args(&s(&["ci", "run", "--prune"])).unwrap();
        assert!(a.flags.iter().any(|f| f == "--prune") && a.opts.is_empty());
        assert_eq!(expand_eq(&s(&["lod", "d.json", "--levels=3"])).unwrap(), s(&["lod", "d.json", "--levels", "3"]));
        assert!(expand_eq(&s(&["ci", "run", "--prune=1"])).is_err());
        assert_eq!(expand_eq(&s(&["render", "--scale", "a=b"])).unwrap(), s(&["render", "--scale", "a=b"]));
    }

    #[test]
    fn unknown_flags_are_refused_with_a_suggestion() {
        let go = |v: &[&str]| {
            let raw: Vec<String> = v.iter().map(|s| s.to_string()).collect();
            check_flags(&parse_args(&raw).unwrap(), &raw)
        };
        let e = go(&["render", "p.json", "0", "o.png", "--bone"]).unwrap_err();
        assert!(e.contains("--bone") && e.contains("did you mean --bones"), "{e}");
        let e = go(&["export-fit", "p.json", "o.json", "--audi"]).unwrap_err();
        assert!(e.contains("--audit"), "{e}");
        assert!(go(&["render", "p.json", "0", "o.png", "--bones"]).is_ok());
        // options are per command: `--json` belongs to the commands whose usage lists it
        let e = go(&["render", "p.json", "0", "o.png", "--json"]).unwrap_err();
        assert!(e.contains("unknown option --json for 'render'"), "{e}");
        assert!(go(&["timeline", "p.json", "--json"]).is_ok());
        // commands whose usage line lists no option at all refuse stray flags too
        for c in [vec!["check", "p.json"], vec!["stats", "p.json"], vec!["lint", "p.json"], vec!["coach", "p.json"], vec!["contracts", "p.json", "c.json"], vec!["machine", "list", "p.json"]] {
            let mut v = c.clone();
            v.push("--foo");
            let e = go(&v).unwrap_err();
            assert!(e.contains("unknown option --foo"), "{c:?}: {e}");
            assert!(go(&c).is_ok(), "{c:?}");
        }
    }

    /// Splits a CLI source file into `(commands, text)` chunks: in `main.rs` one chunk per `["command", ...] =>` arm of
    /// the dispatcher (commands = every `["name"` of the arm's pattern) and `None` for code outside the arms (shared
    /// helpers); other modules are one chunk for the commands they implement.
    fn option_read_sites() -> Vec<(String, Option<Vec<String>>, String)> {
        let strip = |body: &str| -> String {
            let body = body.split("mod usage_tests").next().unwrap().to_string();
            // the option tables (VALUE_OPTS, CMD_OPTS) list names for the parser, they are not reads
            let body = match (body.find("const VALUE_OPTS"), body.find("const GLOBAL_FLAGS")) {
                (Some(a), Some(b)) if a < b => format!("{}{}", &body[..a], &body[b..]),
                _ => body,
            };
            match (body.find("const USAGE"), body.find("\";\n")) {
                (Some(a), Some(_)) => {
                    let end = body[a..].find("\";\n").map(|e| a + e).unwrap_or(a);
                    format!("{}{}", &body[..a], &body[end..])
                }
                _ => body,
            }
        };
        let cmds = |names: &[&str]| Some(names.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let face: Vec<&str> = vec!["face-synth", "face-train", "face-eval", "face-detect", "face-track", "face-calibrate", "face-bake"];
        let mut out = vec![
            ("lod_cmd.rs".to_string(), cmds(&["lod", "lod-pick"]), strip(include_str!("lod_cmd.rs"))),
            ("fit_cmd.rs".to_string(), cmds(&["export-fit", "size-report"]), strip(include_str!("fit_cmd.rs"))),
            ("fuzz_cmd.rs".to_string(), cmds(&["fuzz", "fuzz-grid"]), strip(include_str!("fuzz_cmd.rs"))),
            ("vcs_cmd.rs".to_string(), cmds(&["vcs"]), strip(include_str!("vcs_cmd.rs"))),
            ("face_cmd.rs".to_string(), cmds(&face), strip(include_str!("face_cmd.rs"))),
        ];
        // main.rs: chunk per dispatcher arm
        let main = strip(include_str!("main.rs"));
        let mut cur: (Option<Vec<String>>, String) = (None, String::new());
        let mut in_header = false;
        for l in main.lines() {
            let is_arm = l.starts_with("        [\"");
            let top = !l.is_empty() && !l.starts_with(' ');
            if is_arm || top {
                if !cur.1.is_empty() {
                    out.push(("main.rs".to_string(), cur.0.take(), std::mem::take(&mut cur.1)));
                }
                cur.0 = None;
                in_header = is_arm;
                if is_arm {
                    cur.0 = Some(Vec::new());
                }
            }
            if in_header {
                let mut rest = l;
                while let Some(i) = rest.find("[\"") {
                    let t = &rest[i + 2..];
                    let e = t.find('"').unwrap_or(0);
                    if let Some(v) = cur.0.as_mut() {
                        v.push(t[..e].to_string());
                    }
                    rest = &t[e..];
                }
                if l.contains("=>") {
                    in_header = false;
                }
            }
            cur.1.push_str(l);
            cur.1.push('\n');
        }
        out.push(("main.rs".to_string(), cur.0, cur.1));
        out
    }

    /// Every `"--flag"` literal the CLI sources read is listed in the USAGE of THE COMMAND whose code reads it
    /// (a dispatcher arm of `main.rs`, or the command's own module), not merely somewhere in USAGE. Code shared by
    /// several commands (outside the arms) is checked against the USAGE as a whole.
    #[test]
    fn every_flag_the_code_reads_is_in_the_strict_tables() {
        let all: Vec<String> = usage_forms().iter().flat_map(|f| f.opts.clone()).collect();
        let mut missing: Vec<String> = Vec::new();
        for (file, cmds, body) in option_read_sites() {
            let allowed: Option<Vec<String>> = cmds.filter(|c| !c.is_empty()).map(|c| c.iter().flat_map(|c| documented_options(c)).collect());
            let mut rest = body.as_str();
            while let Some(i) = rest.find("\"--") {
                let tail = &rest[i + 1..];
                let name = &tail[..tail.find('"').unwrap_or(0)];
                rest = &tail[name.len().max(1)..];
                if name.len() <= 2 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || GLOBAL_FLAGS.contains(&name) || GLOBAL_OPTS.contains(&name) {
                    continue;
                }
                let ok = match &allowed {
                    Some(a) => a.iter().any(|o| o == name),
                    None => all.iter().any(|o| o == name),
                };
                let label = match &allowed { Some(_) => file.clone(), None => format!("{file} (shared code)") };
                let entry = format!("{label}: {name}");
                if !ok && !missing.contains(&entry) {
                    missing.push(entry);
                }
            }
        }
        assert!(missing.is_empty(), "flags read by a command but not in that command's USAGE: {missing:?}");
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn refused(v: &[&str]) -> Option<String> {
        let raw = args(v);
        let expanded = expand_eq(&raw).unwrap();
        check_flags(&parse_args(&expanded).unwrap(), &expanded).err()
    }

    /// Flag validation is per synopsis form: an option listed for one sub-command is refused on its siblings.
    #[test]
    fn options_are_validated_per_sub_command() {
        // ci: --quiet only for `run`, --out/--no-goldens only for `init`, --metric only for `diff`
        assert!(refused(&["ci", "run", "--quiet"]).is_none());
        assert!(refused(&["ci", "diff", "a.png", "b.png", "--quiet"]).unwrap().contains("unknown option --quiet for 'ci diff'"));
        assert!(refused(&["ci", "init", "s.json", "--quiet"]).unwrap().contains("--quiet"));
        assert!(refused(&["ci", "init", "s.json", "--no-goldens"]).is_none());
        assert!(refused(&["ci", "run", "--no-goldens"]).is_some());
        assert!(refused(&["ci", "diff", "a.png", "b.png", "--metric", "mse", "--tol", "3"]).is_none());
        // trail: --json only on the --target form
        assert!(refused(&["trail", "s.json", "hand", "t.svg", "--json"]).unwrap().contains("--json"));
        assert!(refused(&["trail", "s.json", "t.svg", "--target", "bone:hand", "--json"]).is_none());
        assert!(refused(&["trail", "s.json", "hand", "t.svg", "--from", "1", "--to", "5"]).is_none());
        // ui: --json for commands|shortcuts only
        assert!(refused(&["ui", "layout", "presets", "--json"]).unwrap().contains("--json"));
        assert!(refused(&["ui", "search", "x", "--json"]).is_some());
        assert!(refused(&["ui", "commands", "--json"]).is_none());
        assert!(refused(&["ui", "shortcuts", "--json", "--ui-config", "f.json"]).is_none());
        assert!(refused(&["ui", "layout", "show", "rig", "--ui-config", "f.json"]).is_some());
        // golden: --scale only for write, --tol only for check (value options are validated too)
        assert!(refused(&["golden", "write", "p.json", "g.json", "--tol", "0.1"]).unwrap().contains("--tol"));
        assert!(refused(&["golden", "check", "p.json", "g.json", "--scale", "0.25"]).unwrap().contains("--scale"));
        assert!(refused(&["golden", "write", "p.json", "g.json", "--scale", "0.25"]).is_none());
        assert!(refused(&["golden", "check", "p.json", "g.json", "--tol", "0.1"]).is_none());
        // vcs: --json only where the sub-command prints JSON; --force only for checkout
        assert!(refused(&["vcs", "status", "--json"]).is_none());
        assert!(refused(&["vcs", "commit", "-m", "x", "--json"]).is_some());
        assert!(refused(&["vcs", "gc", "--json"]).is_some());
        assert!(refused(&["vcs", "log", "--force"]).is_some());
        assert!(refused(&["vcs", "checkout", "main", "--force"]).is_none());
        assert!(refused(&["vcs", "merge", "dev", "--no-ff", "--repo", "r"]).is_none());
        assert!(refused(&["vcs", "revert", "abc", "--ours"]).is_some());
        // keyedit: each edit lists its own options
        assert!(refused(&["keyedit", "p.json", "o.json", "move", "--sel", "a:b", "--dt", "2", "--free"]).is_none());
        assert!(refused(&["keyedit", "p.json", "o.json", "delete", "--sel", "a:b", "--dt", "2"]).unwrap().contains("--dt"));
        assert!(refused(&["keyedit", "p.json", "o.json", "smooth", "--sel", "a:b", "--kind", "monotone"]).is_none());
        assert!(refused(&["keyedit", "p.json", "o.json", "ease", "--sel", "a:b", "--ease", "linear", "--kind", "x"]).is_some());
        // pose-suggest / pose-db / clip / sound / bind / container / scroll sub-forms
        assert!(refused(&["pose-suggest", "next", "p.json", "-", "--frame", "1", "--json"]).is_some());
        assert!(refused(&["pose-suggest", "between", "p.json", "-", "--a", "1", "--b", "2", "--json"]).is_none());
        assert!(refused(&["pose-db", "info", "d.json", "--step", "2"]).is_some());
        assert!(refused(&["clip", "list", "p.json", "--at", "2"]).is_some());
        assert!(refused(&["sound", "remove", "p.json", "o.json", "n", "--gain", "2"]).is_some());
        assert!(refused(&["sound", "edit", "p.json", "o.json", "n", "--gain", "2", "--fade-in", "1"]).is_none());
        assert!(refused(&["container", "list", "p.json", "--size", "10x10"]).is_some());
        assert!(refused(&["container", "solve", "p.json", "--size", "10x10"]).is_none());
        assert!(refused(&["scroll", "list", "p.json", "--offset", "1,1"]).is_some());
        assert!(refused(&["bind-eval", "p.json", "v.json", "--data", "o.json"]).is_none());
        assert!(refused(&["bind", "list", "p.json", "--expr", "x"]).is_some());
        // onion: the two forms
        assert!(refused(&["onion", "p.json", "0", "o.png", "--keyed"]).is_some());
        assert!(refused(&["onion", "p.json", "o.svg", "--frame", "1", "--keyed"]).is_none());
        // keys / idle / comp / snapshot: one option set per sub-command
        assert!(refused(&["keys", "shift", "p.json", "o.json", "--by", "2", "--target", "a", "--prop", "bone_angle", "--from", "1", "--to", "5"]).is_none());
        assert!(refused(&["keys", "shift", "p.json", "o.json", "--by", "2", "--factor", "2"]).unwrap().contains("unknown option --factor for 'keys shift'"));
        assert!(refused(&["keys", "scale", "p.json", "o.json", "--factor", "2", "--pivot", "0"]).is_none());
        assert!(refused(&["keys", "scale", "p.json", "o.json", "--factor", "2", "--ease", "linear"]).unwrap().contains("applies to: keys ease"));
        assert!(refused(&["keys", "ease", "p.json", "o.json", "--ease", "smooth", "--by", "1"]).unwrap().contains("--by"));
        assert!(refused(&["keys", "reverse", "p.json", "o.json", "--from", "1", "--pivot", "3"]).unwrap().contains("--pivot"));
        assert!(refused(&["idle", "blink", "p.json", "o.json", "--target", "a", "--prop", "layer_switch", "--interval", "2", "--seed", "3"]).is_none());
        assert!(refused(&["idle", "blink", "p.json", "o.json", "--target", "a", "--prop", "p", "--amp", "2"]).unwrap().contains("unknown option --amp for 'idle blink'"));
        assert!(refused(&["idle", "breath", "p.json", "o.json", "--target", "a", "--prop", "p", "--seed", "2"]).unwrap().contains("--seed"));
        assert!(refused(&["idle", "breath", "p.json", "o.json", "--target", "a", "--prop", "p", "--period", "40"]).is_none());
        assert!(refused(&["idle", "sway", "p.json", "o.json", "--target", "a", "--prop", "p", "--period", "40"]).unwrap().contains("applies to: idle breath"));
        assert!(refused(&["idle", "sway", "p.json", "o.json", "--target", "a", "--prop", "p", "--freq", "1", "--seed", "4"]).is_none());
        assert!(refused(&["comp", "list", "p.json", "--name", "x"]).is_some());
        assert!(refused(&["comp", "capture", "p.json", "o.json", "n"]).is_none());
        assert!(refused(&["snapshot", "save", "p.json", "d", "--keep", "3"]).is_none());
        assert!(refused(&["snapshot", "list", "p.json", "d", "--keep", "3"]).unwrap().contains("applies to: snapshot save"));
        assert!(refused(&["snapshot", "restore", "p.json", "d", "o.json", "--number", "2"]).is_none());
        assert!(refused(&["snapshot", "save", "p.json", "d", "--number", "2"]).unwrap().contains("applies to: snapshot restore"));
        assert!(refused(&["snapshot", "recover", "p.json", "d", "o.json", "--number", "2"]).is_some());
        // export: the output extension picks the form, and an option that the format does not use is refused
        assert!(refused(&["export", "p.json", "o.gif", "--scale", "0.5", "--report", "r.json"]).is_none());
        assert!(refused(&["export", "p.json", "O.GIF", "--scale", "0.5"]).is_none());
        let e = refused(&["export", "p.json", "o.json", "--scale", "0.5"]).unwrap();
        assert!(e.contains("unknown option --scale for 'export .json'") && e.contains("applies to: export .gif"), "{e}");
        assert!(refused(&["export", "p.json", "o.lottie", "--report", "r.json"]).is_none());
        assert!(refused(&["export", "p.json", "o.html", "--report", "r.json"]).unwrap().contains("--report"));
        assert!(refused(&["export", "p.json", "o.htm", "--scale", "2"]).is_some());
        assert!(refused(&["export", "p.json", "o.mp4", "--scale", "2"]).is_none());
        assert!(refused(&["export", "p.json", "o.mp4", "--codec", "av1"]).unwrap().contains("applies to: export .webm"));
        assert!(refused(&["export", "p.json", "o.webm", "--codec", "av1", "--scale", "2"]).is_none());
        assert!(refused(&["export", "p.json", "o.apng", "--columns", "2"]).unwrap().contains("--columns"));
        assert!(refused(&["export", "p.json", "o.png", "--columns", "2", "--scale", "0.5"]).is_none());
        assert!(refused(&["export", "p.json", "o.glb", "--scale", "2"]).is_some());
        assert!(refused(&["export", "p.json", "o.glb", "--report", "r.json"]).is_none());
        // an unknown or missing extension: every export option is let through, the command reports the format error
        assert!(refused(&["export", "p.json", "o.xyz", "--scale", "2"]).is_none());
        // the error names the sub-command that does accept the option
        let e = refused(&["ci", "diff", "a.png", "b.png", "--quiet"]).unwrap();
        assert!(e.contains("applies to: ci run"), "{e}");
    }

    /// The first form of every USAGE line is parsed into the options the docs and the validation use; a few anchors.
    #[test]
    fn usage_forms_split_alternatives() {
        let f: Vec<&UsageForm> = usage_forms().iter().filter(|f| f.cmd == "ci").collect();
        assert_eq!(f.len(), 4, "{f:?}");
        assert_eq!(f.iter().map(|x| x.keys[0].as_str()).collect::<Vec<_>>(), ["init", "run", "bless", "diff"]);
        assert!(select_forms("golden", &args(&["golden", "write"]), &[]).iter().all(|f| f.keys == ["write"]));
        assert_eq!(usage_forms().iter().filter(|f| f.cmd == "keyedit").count(), 16);
        assert_eq!(usage_forms().iter().filter(|f| f.cmd == "keys").count(), 4);
        assert_eq!(usage_forms().iter().filter(|f| f.cmd == "idle").count(), 3);
        assert_eq!(usage_forms().iter().filter(|f| f.cmd == "snapshot").count(), 4);
        assert_eq!(usage_forms().iter().filter(|f| f.cmd == "comp").count(), 2);
        let ex: Vec<&UsageForm> = usage_forms().iter().filter(|f| f.cmd == "export").collect();
        assert_eq!(ex.len(), 8);
        // every extension the dispatcher accepts has exactly one export form
        for e in ["gif", "html", "htm", "json", "lottie", "mp4", "apng", "webm", "png", "glb"] {
            assert_eq!(ex.iter().filter(|f| f.exts.iter().any(|x| x == e)).count(), 1, "{e}");
        }
        assert!(usage_forms().iter().any(|f| f.cmd == "bind-eval"));
    }

    /// CLI_REFERENCE.md: every option of a command's USAGE appears in that command's synopsis lines of the reference
    /// (and no synopsis line there names an option the USAGE of the command does not have).
    #[test]
    fn cli_reference_synopses_match_usage() {
        let doc = include_str!("../../docs/CLI_REFERENCE.md");
        // synopsis lines: bold-label lines (`**Synopsis:** `...``) and table cells with a backticked `armatura <cmd> ...`, and
        // bare `armatura <cmd> ...` lines inside code fences with their indented continuation lines (never `$ ` examples)
        let mut syn: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        let mut fence = false;
        let mut last: Option<(String, usize)> = None; // the fence piece an indented continuation line extends
        for l in doc.lines() {
            if l.trim_start().starts_with("```") {
                fence = !fence;
                last = None;
                continue;
            }
            let t = l.trim();
            let mut pieces: Vec<&str> = Vec::new();
            if fence {
                if t.starts_with("armatura ") {
                    pieces.push(t);
                } else if l.starts_with(char::is_whitespace) && !t.starts_with('$') {
                    if let Some((c, i)) = &last {
                        let v = syn.get_mut(c).unwrap();
                        v[*i].push(' ');
                        v[*i].push_str(t);
                    }
                    continue;
                } else {
                    last = None;
                }
            } else if t.starts_with("**") || t.starts_with('|') {
                let mut rest = t;
                while let Some(i) = rest.find("`armatura ") {
                    let s = &rest[i + 1..];
                    let e = s.find('`').unwrap_or(s.len());
                    pieces.push(&s[..e]);
                    rest = &s[e..];
                }
            }
            for p in pieces {
                if let Some(cmd) = p.split_whitespace().nth(1) {
                    let v = syn.entry(cmd.to_string()).or_default();
                    v.push(p.to_string());
                    last = if fence { Some((cmd.to_string(), v.len() - 1)) } else { None };
                }
            }
        }
        let mut problems: Vec<String> = Vec::new();
        let mut cmds: Vec<String> = usage_forms().iter().map(|f| f.cmd.clone()).collect();
        cmds.sort();
        cmds.dedup();
        for cmd in &cmds {
            let usage_opts = documented_options(cmd);
            let Some(lines) = syn.get(cmd) else {
                // commands with no synopsis line of their own (sub-forms of another command's section)
                if !usage_opts.is_empty() {
                    problems.push(format!("{cmd}: no synopsis line in CLI_REFERENCE.md"));
                }
                continue;
            };
            let text = lines.join(" ").replace("\\|", "|");
            let doc_opts = option_names(&text, false);
            for o in &usage_opts {
                if !doc_opts.contains(o) {
                    problems.push(format!("{cmd}: {o} is in USAGE but not in the CLI_REFERENCE.md synopsis"));
                }
            }
            for o in &doc_opts {
                if !usage_opts.contains(o) && !GLOBAL_FLAGS.contains(&o.as_str()) && !GLOBAL_OPTS.contains(&o.as_str()) {
                    problems.push(format!("{cmd}: {o} is in the CLI_REFERENCE.md synopsis but not in USAGE"));
                }
            }
        }
        assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
    }

    /// Every USAGE line that has no alternatives appears word for word (white space aside) in CLI_REFERENCE.md.
    #[test]
    fn cli_reference_repeats_usage_lines_verbatim() {
        let norm = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ").replace("\\|", "|");
        let doc = norm(include_str!("../../docs/CLI_REFERENCE.md"));
        let mut missing = Vec::new();
        for l in USAGE.lines().filter(|l| l.starts_with("  armatura ")) {
            let body = l.trim().split("   ").next().unwrap();
            if body.starts_with("armatura --") || split_alternatives(body).len() > 1 {
                continue;
            }
            if !doc.contains(&norm(body)) {
                missing.push(body.to_string());
            }
        }
        assert!(missing.is_empty(), "USAGE lines missing from CLI_REFERENCE.md:\n{}", missing.join("\n"));
    }

    /// `golden write --scale` above 1 is refused (it used to be clamped silently); `golden check --scale` is refused.
    #[test]
    fn golden_scale_rules() {
        assert!(refused(&["golden", "check", "p.json", "g.json", "--scale", "0.5"]).is_some());
        assert!(usage_for("golden").unwrap().contains("(0, 1]"));
    }

    /// C2: every option in USAGE must parse the way the documentation says: `[--x]` is a boolean
    /// flag, `[--x VALUE]` takes a value.
    #[test]
    fn every_documented_option_parses_as_documented() {
        // join continuation lines to their command
        let mut cmds: Vec<String> = Vec::new();
        for l in USAGE.lines() {
            if l.starts_with("  armatura ") {
                // drop the padding after the command name and the prose after the gap (padded names such as `ci       init`
                // used to cut the line short, so those commands were never checked)
                let t = l.trim();
                let mut it = t.split_whitespace();
                let (a0, a1) = (it.next().unwrap(), it.next().unwrap());
                let rest = t[t.find(a1).unwrap() + a1.len()..].trim_start();
                cmds.push(format!("{a0} {a1} {}", rest.split("   ").next().unwrap()));
            } else if l.starts_with("      ") && !cmds.is_empty() {
                let last = cmds.len() - 1;
                cmds[last].push(' ');
                cmds[last].push_str(l.trim().split("   ").next().unwrap());
            }
        }
        assert!(cmds.len() > 50);
        let mut checked = 0;
        let mut bad: Vec<String> = Vec::new();
        for c in &cmds {
            let toks: Vec<&str> = c.split_whitespace().collect();
            let cmd = toks[1];
            if cmd.starts_with('-') {
                continue;
            }
            for (i, t) in toks.iter().enumerate() {
                let name = t.trim_start_matches('[').trim_end_matches([']', ',']);
                let name = name.split('|').next().unwrap();
                if !name.starts_with("--") || name.len() < 3 || name.contains('<') || name.contains('=') {
                    continue;
                }
                // value if the next token exists and is a placeholder (not a closing/another option)
                let closes = t.ends_with(']');
                let next = toks.get(i + 1).copied().unwrap_or("");
                let takes_value = !closes && !next.is_empty() && !next.starts_with('[') && !next.starts_with("--") && !next.starts_with('(') && !next.starts_with('<') || (!closes && next.starts_with('<'));
                let args: Vec<String> = [cmd, name, "2", "after"].iter().map(|s| s.to_string()).collect();
                let Ok(a) = parse_args(&args) else { panic!("{cmd} {name}: parse failed") };
                if takes_value {
                    if a.opts.get(name).map(String::as_str) != Some("2") {
                        bad.push(format!("{cmd} {name}: documented with a value but parsed as a flag"));
                    }
                } else {
                    if !(a.flags.iter().any(|f| f == name) && a.pos.iter().any(|p| p == "2")) {
                        bad.push(format!("{cmd} {name}: documented as a boolean flag but swallows the next argument"));
                    }
                }
                checked += 1;
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
        assert!(checked > 100, "only {checked} options checked");
    }
}
