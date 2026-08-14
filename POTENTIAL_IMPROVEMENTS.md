# Potential Rust improvements for `mtp-tui`

Review date: 2026-08-15  
Reviewed revision: `601f3c4` (`Fix navigation lag`)

## Scope and assumptions

This is a repository-specific review of `mtp-tui`, informed by the Rust design notes in [`crabbook`](https://github.com/Nekrolm/crabbook/). It is a proposal, not an implementation plan that has already been validated on hardware.

Assumptions:

- Device metadata is an external input and must not be trusted as a safe host filename.
- The TUI should remain responsive while an MTP operation is in progress.
- A failed or cancelled transfer should preserve the pre-operation file whenever the device and host filesystem make that possible.
- Large files should not require memory proportional to their size.
- Stable Rust and safe Rust are preferred. No current requirement justifies introducing application-level `unsafe`.
- Supporting non-UTF-8 host filenames and atomic replacement on every platform are product decisions, not facts assumed by this review.

## Verified baseline

The following checks passed on the reviewed revision:

- `cargo test --all-targets`: 21 unit tests and 2 CLI integration tests passed; the example target compiled and had no tests.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- A source scan found no direct `unsafe` blocks or unsafe implementations in `src`, `tests`, or `examples`.

These checks do **not** establish correct behavior with a real MTP device. No device transfer, disconnect, short read/write, disk-full, cancellation, panic, signal, non-UTF-8 filename, or multi-platform replacement scenario was exercised during this review.

## What is already well designed

Several choices align well with the crabbook guidance and should be preserved:

- `DeviceBackend: Send`, without a `Sync` requirement, accurately expresses that backend ownership moves to a worker and is not shared concurrently (`src/backend.rs:14`). This follows the distinction in [`send_and_sync.md`](https://github.com/Nekrolm/crabbook/blob/master/send_and_sync.md).
- The backend itself is sent through channels and returned on completion (`src/types.rs:21-33`, `src/types.rs:94-100`). This avoids an unnecessary `Arc<Mutex<_>>` around MTP state and matches the single-owner/event-loop approach in [`event_loops_and_shared_state.md`](https://github.com/Nekrolm/crabbook/blob/master/event_loops_and_shared_state.md).
- Most internal APIs accept `&str` and `&Path`, rather than `&String` and `&PathBuf` (`src/backend.rs:14-46`). That is the flexible borrowed boundary recommended by [`borrowed_args.md`](https://github.com/Nekrolm/crabbook/blob/master/borrowed_args.md).
- `Box<dyn DeviceBackend>` is a reasonable dynamic boundary here. It supports a fake backend and avoids spreading backend type parameters and monomorphization through the application, consistent with the trade-offs discussed in [`impl_trait_compilation_blow.md`](https://github.com/Nekrolm/crabbook/blob/master/impl_trait_compilation_blow.md).
- The workflow is represented with enums (`DeviceState`, `ActiveDialog`, `TransferKind`) instead of independent Boolean flags (`src/types.rs:43-116`). This already rules out many invalid combinations.

## Priority summary

| Priority | Improvement | Main risk addressed | Works when / caveat |
|---|---|---|---|
| P0 | Validate device filenames before joining them to a host directory | Path escape or unintended overwrite | Validation must follow the target OS path rules and accept exactly one normal filename component |
| P0 | Stage host downloads and commit only after success | Truncated/partial destination after a failed pull | Atomic commit requires a temporary file on the same filesystem; replacement semantics differ by OS |
| P0 | Stream uploads without collecting the entire file | Memory proportional to file size; repeated read-error loop | The `mtp-rs` stream must be able to own or safely borrow the reader for the upload duration |
| P0 | Redesign overwrite push as a recoverable operation | Existing device file is deleted before upload succeeds | Strong guarantees depend on device support for temporary upload, rename, and cleanup |
| P1 | Use one owned device worker for all MTP calls | UI stalls, detached threads, awkward ownership transitions | MTP operations must be serialized for the selected device/storage |
| P1 | Define cancellation and shutdown semantics | Process exits while a transfer owns the backend | True cancellation depends on `mtp-rs`/device support; otherwise expose “finish then quit” clearly |
| P1 | Stop silently discarding configuration and directory-entry errors | Incomplete listings and ignored configuration mistakes | Partial listings may remain useful if accompanied by visible warnings |
| P1 | Model backend capabilities and typed operation outcomes | “Not implemented” discovered only after an action | Capabilities can be cached after connecting and used to disable unsupported UI actions |
| P2 | Reduce avoidable cloning and interior mutability only after measurement | Listing memory/CPU and API clarity | Worth doing for very large directories; current simplicity may be preferable for normal sizes |
| P2 | Tighten dependencies, module boundaries, and tests | Build cost and maintainability | Remove features/dependencies only after all targets and supported platforms are verified |

## Detailed recommendations

### P0 — Treat a device filename as untrusted input

`pull_file` constructs a local path with `target_dir.join(filename)` and immediately creates it (`src/backend.rs:250-264`). There is no check that `filename` is a single normal path component.

If an MTP device can report a name such as `../outside`, an absolute path, or a platform-specific prefix/separator form, joining can address a location outside the selected host directory. Even if common devices sanitize names, the backend boundary should enforce this invariant locally.

Recommended invariant:

1. Parse the device-supplied name according to the target platform.
2. Accept exactly one non-empty `Component::Normal` component.
3. Reject root, prefix, parent, current-directory, embedded separator, and NUL/control forms that the host API or UI cannot represent safely.
4. Return a user-visible error before opening any host file.
5. Keep the trusted local target as a `PathBuf`; use the display name only for UI text.

This works when the intended product behavior is “download into the current host directory under one filename.” If preserving arbitrary device names is required, define an explicit escaping/mapping scheme and collision policy instead of passing them directly to the host filesystem.

### P0 — Make pulls failure-atomic where the filesystem permits

`File::create` truncates an existing destination before the first download chunk is written (`src/backend.rs:262-275`). A device disconnect, disk-full error, failed flush, panic, or forced quit can therefore leave a partial file and destroy the previous content.

Prefer a small transaction:

1. Validate the final filename.
2. Create an unpredictable temporary file with `create_new(true)` in the destination directory.
3. Stream into it.
4. Flush and, if durability is part of the contract, call `sync_all`.
5. Commit by renaming/replacing the temporary file.
6. Remove the temporary file on every non-commit path with a focused cleanup guard.

Creating the temporary file in the destination directory is important: a rename is normally atomic only within the same filesystem. Replacement behavior is not identical on Unix and Windows, and no approach survives `SIGKILL`, power loss, or filesystem corruption without additional filesystem-specific work. The code and UI should state the guarantee it actually provides.

This recommendation is the practical RAII application from [`raii_and_memory_safety.md`](https://github.com/Nekrolm/crabbook/blob/master/raii_and_memory_safety.md), with the warning from [`you_dont_want_drop.md`](https://github.com/Nekrolm/crabbook/blob/master/you_dont_want_drop.md): prefer a narrow, proven guard/scope mechanism over a broad custom `Drop` implementation on `App` or the backend.

### P0 — Stream uploads and terminate after the first read error

`push_file` currently builds `Vec<Result<Bytes, io::Error>>` with `.collect()` before starting the upload (`src/backend.rs:206-225`). This has two consequences:

- Memory grows approximately with the entire file size, plus per-chunk allocation and vector overhead.
- The `from_fn` closure returns `Some(Err(e))` on an I/O error but does not mark the iterator finished. A persistent read error can therefore produce errors indefinitely while `.collect()` keeps allocating.

Replace the eager vector with a lazy stream that owns the `BufReader`, yields one bounded chunk at a time, and transitions permanently to EOF after its first read error. Reuse a buffer where the receiving API permits it. Add upload progress and cancellation checks while making this change.

The important condition is the accepted stream lifetime in `mtp-rs::upload_with_progress`. If the API requires an owned or `'static` stream, move the reader into an owned stream/state machine or bridge it with a bounded channel; do not use lifetime extension or other unsafe tricks. The ownership trade-offs in [`consume_and_borrowing.md`](https://github.com/Nekrolm/crabbook/blob/master/consume_and_borrowing.md) favor in-place/borrowed processing for large data, but correctness of the stream lifetime comes first.

### P0 — Do not delete the old device file before the replacement is ready

The overwrite path deletes the existing object and only then calls `push_file` (`src/app.rs:822-840`). If upload fails, the original is already gone. The backend also reports that a partial uploaded object may remain, but does not attempt cleanup (`src/backend.rs:229-243`).

Preferred sequence, if the device supports it:

1. Upload under a unique temporary device name.
2. Verify completion and refresh device metadata.
3. Delete or move aside the old object.
4. Rename the new object to the requested name.
5. Attempt cleanup/rollback for every failure point and report any residue precisely.

There may be no truly atomic MTP replacement primitive. If the device cannot upload and rename safely, the UI should not promise “overwrite.” Safer alternatives are “delete then upload (original can be lost),” “upload under a different name,” or refusing the operation. This is a capability-dependent product choice.

### P1 — Give the device one long-lived owner thread

Listings and transfers spawn detached threads and move the backend out and back through result messages (`src/app.rs:69-95`, `src/app.rs:788-952`). This is much better than shared mutable backend state, but it leaves several issues:

- `delete`, `mkdir`, `rename`, and inspection execute synchronously on the TUI thread (`src/app.rs:342-370`, `src/app.rs:644-674`, `src/app.rs:757-785`). A slow device can freeze input and rendering.
- `JoinHandle`s are discarded, so shutdown cannot wait for a worker or distinguish worker panic from channel teardown.
- State transitions temporarily install `Disconnected` as a sentinel and then rely on `unreachable!()` (`src/app.rs:232-247`, `src/app.rs:788-799`). The type does not fully encode ownership in transit.
- Every operation reconstructs thread/message plumbing and duplicates recovery logic.

Use one worker that owns `Box<dyn DeviceBackend>` (or constructs it on startup), receives `DeviceCommand` values over a bounded channel, and emits `DeviceEvent` values. All MTP calls—including metadata inspection and mutations—go through it. The UI event loop remains the sole owner of UI state and renders progress/events in order.

This preserves the strongest existing design decision: the backend is `Send`, not shared and not required to be `Sync`. It directly follows [`event_loops_and_shared_state.md`](https://github.com/Nekrolm/crabbook/blob/master/event_loops_and_shared_state.md) and [`send_and_sync.md`](https://github.com/Nekrolm/crabbook/blob/master/send_and_sync.md).

Use a bounded command queue or allow only one in-flight operation. An unbounded queue is harmless at today's keyboard rate but becomes a memory/backpressure problem if batch operations or automated input are added.

### P1 — Define graceful cancellation and shutdown

During a transfer, Ctrl-C sets `should_quit` and the main loop exits (`src/app.rs:271-276`, `src/app.rs:141-148`). The detached transfer thread may still own the backend and be writing when process teardown stops it. The confirmation-based quit path has the same result once confirmed. There is no join or cooperative cancellation protocol.

Choose and document one behavior:

- **Cooperative cancel:** request cancellation, let upload/download code stop at a chunk boundary, clean partial artifacts, return the backend, then exit.
- **Finish then quit:** disable immediate exit, continue rendering progress, and quit after the operation completes.
- **Force quit:** retain an explicit second-step escape hatch and warn that partial files/objects may remain.

A cancellation token only works if every long operation observes it and the underlying library call can make progress back to the check. If a device/USB call can block forever, a timeout may inform the UI but cannot safely reclaim a thread or backend; process termination remains the last resort.

Also add terminal cleanup for panic/unwind paths. The current explicit `ratatui::restore()` runs after normal `Result` returns (`src/main.rs:47-50`), but not after a panic or abort. A narrow terminal-session guard and panic hook can cover unwinding; no Rust destructor can cover `abort`, `SIGKILL`, or abrupt power loss.

### P1 — Preserve useful errors instead of converting them to absence

Several boundaries discard error information:

- Invalid/unreadable config becomes the default config, and template creation failures are ignored (`src/config.rs:37-53`).
- Host directory iteration and metadata failures silently drop entries (`src/app.rs:956-981`).
- Host list refresh failures are ignored in some UI paths (`src/app.rs:440-455`, `src/app.rs:894-903`).
- A single `ListingItem::Skipped` fails the complete device listing (`src/backend.rs:146-156`), which is the opposite extreme.
- Device refresh errors after mutations/uploads are ignored (`src/backend.rs:246`, `src/backend.rs:285` and similar calls).

Return structured results such as `{ entries, warnings }` for partial listings. Show a concise status and retain detailed context for an inspector/log. For config, distinguish “not present” from “present but invalid/unreadable”; defaulting is reasonable only when accompanied by a warning for the latter.

Whether a skipped MTP object should fail the whole listing is a product choice. A file manager usually benefits from a partial list plus warnings, but a batch/verification mode may require fail-closed behavior.

### P1 — Make capabilities and outcomes explicit

`DeviceBackend` provides default operation methods that fail at runtime with “not implemented yet” (`src/backend.rs:26-43`). Rename alone checks a device capability inside the concrete backend. This makes the UI discover unsupported actions only after the user invokes them.

Add a `DeviceCapabilities` value after connection and use it to disable or explain unsupported operations. Prefer typed outcomes for expected states—unsupported, disconnected, conflict, cancelled, partial object remaining—while retaining `anyhow::Context` at the application/reporting edge for unexpected error chains.

This works best if capability queries are stable for a connection. If devices change behavior per storage or object type, scope capabilities accordingly and still handle an operation-level rejection.

### P2 — Refine borrowing and cloning based on measurements

The code duplicates every successful device listing into `device_raw_entries` and a filtered `device_pane.entries` (`src/app.rs:199-206`), then clones the raw list again whenever hidden items are toggled (`src/app.rs:457-465`). This is simple and may be entirely acceptable for ordinary phone directories.

If profiles show large-list cost, keep one owned entry vector and expose a filtered index/view, or rebuild one visible vector while preserving the selected name before replacement. Avoid reaching immediately for shared ownership/interior mutability; the event loop already has exclusive access to `App`.

One correctness detail can be fixed independently of profiling: the device hidden-file toggle captures the selected name **after** replacing the visible entries, so it may preserve the item now occupying the old numeric index rather than the previously selected item (`src/app.rs:457-465`). Capture the name first, then replace/filter, then restore by name.

The progress callback can also express its mutation directly. `list_current_dir_with_progress` accepts `&dyn Fn`, forcing `Cell<Option<Instant>>` in the caller (`src/backend.rs:17-20`, `src/app.rs:928-943`). If callbacks are sequential, accept `&mut dyn FnMut` and use an ordinary mutable `Option<Instant>`. This narrows the contract and removes unnecessary interior mutability. It only works if `mtp-rs` never invokes that callback concurrently; otherwise keep a synchronized design and document the requirement.

### P2 — Decide the host filename model explicitly

Host names are converted with `to_string_lossy()` (`src/app.rs:974-976`, `src/backend.rs:196-201`). On Unix, distinct non-UTF-8 names can display identically after replacement characters, and overwrite matching by display string can become ambiguous.

Options:

- Keep the identity as `OsString`/`PathBuf` and derive a lossy string only for rendering.
- Explicitly support UTF-8 names only and reject unsupported names with a visible diagnostic.

The first option is more faithful; the second is simpler. Whichever is chosen, compare file identity using the native path/name type, not a lossy display value.

### P2 — Tighten build surface and module boundaries

Source usage suggests two build-surface checks:

- The application uses `tokio::runtime::Runtime`, while `Cargo.toml` enables Tokio's `full` feature set (`Cargo.toml:15`). Select only the runtime/features required by the application and example.
- No Rust source directly names `nusb`, and no test/example directly names the `rusb` dev-dependency (`Cargo.toml:12`, `Cargo.toml:20`). Confirm whether they are intentional feature-unification/platform dependencies; if not, remove them.

Validate each dependency change with all targets, a clean build, supported OS builds, and a real-device smoke test. A source scan alone is insufficient because features can affect transitive dependencies.

`src/app.rs` is over 1,000 lines and mixes event polling, commands, transfer orchestration, navigation, dialogs, and host I/O. Split by responsibility after the worker protocol stabilizes—for example, `device_worker`, `commands`, `navigation`, and `transfer`. Keep state transitions centralized so modularization does not turn into shared mutable state or callback indirection.

## Proposed validation matrix

Before claiming the improvements correct, add tests that cover more than the happy path:

| Area | Required scenarios |
|---|---|
| Filename boundary | normal Unicode name; empty name; `.`; `..`; parent traversal; absolute/rooted name; embedded separator; platform prefix; duplicate mapped names |
| Host pull | new target; overwrite; download fails before first chunk; fails mid-stream; write fails/disk full; flush/sync fails; commit fails; existing file remains unchanged; temporary file cleanup |
| Device push | empty file; file larger than memory budget; short reads; interrupted read; persistent read error terminates once; upload failure with and without partial handle; cleanup failure |
| Overwrite push | temporary upload failure; old-object delete failure; rename failure; rollback/cleanup failure; device lacks rename |
| Worker lifecycle | command while busy; worker panic; channel disconnect; device disconnect; cancellation; quit while connecting/listing/transferring; completion arriving near cancellation |
| Listings/config | unreadable entry among readable entries; skipped MTP metadata; malformed TOML; unreadable config; unwritable config directory; refresh failure |
| UI state | selection survives filter/refresh; selected item disappears; empty list; stale completion event; unsupported capability action |
| Platform behavior | Unix and Windows path rules; replacement semantics; non-UTF-8 Unix filename policy; terminal cleanup on unwind and catchable signals |

Use a deterministic fake backend with failure injection for most tests. Keep a small opt-in hardware suite for behaviors that cannot be simulated faithfully. Property-based tests are a good fit for filename validation and state-transition sequences, but ordinary table-driven tests should establish the core contract first.

## Suggested implementation order

1. Define and test filename validation plus staged host-file commit behavior.
2. Replace eager upload collection with a bounded streaming adapter and fault-injection tests.
3. Define honest overwrite semantics from the capabilities exposed by `mtp-rs` and actual target devices.
4. Introduce the single-owner device worker, move every blocking MTP command to it, and add shutdown/cancellation events.
5. Preserve warnings from config, host listing, device listing, and refresh operations.
6. Add capability-aware UI behavior and typed expected outcomes.
7. Fix selection restoration, then profile before redesigning list storage or other cloning.
8. Minimize dependency features and split modules, verifying after each mechanical change.

## Crabbook applicability map

| Essay | Applicability to `mtp-tui` | Takeaway used here |
|---|---|---|
| [`borrowed_args.md`](https://github.com/Nekrolm/crabbook/blob/master/borrowed_args.md) | High | Keep borrowed API parameters (`&str`, `&Path`, slices) unless ownership transfer is required |
| [`impl_trait_references.md`](https://github.com/Nekrolm/crabbook/blob/master/impl_trait_references.md) | Medium | Avoid adding owned boxes or generic parameters merely to escape lifetime/API design issues |
| [`crafting_reference_to_owned.md`](https://github.com/Nekrolm/crabbook/blob/master/crafting_reference_to_owned.md) | Low/guardrail | Do not manufacture ownership or extend lifetimes to satisfy a streaming API |
| [`raii_and_memory_safety.md`](https://github.com/Nekrolm/crabbook/blob/master/raii_and_memory_safety.md) | High | Use ownership and narrow guards for temporary-file cleanup, but specify what happens if cleanup is bypassed or the process is killed |
| [`borrowing_in_generic_functions.md`](https://github.com/Nekrolm/crabbook/blob/master/borrowing_in_generic_functions.md) | Medium | Keep callback lifetimes/contracts simple; do not over-generalize the internal worker protocol |
| [`unsafe_is_unsafe.md`](https://github.com/Nekrolm/crabbook/blob/master/unsafe_is_unsafe.md) | High/guardrail | Preserve local reasoning and safe Rust; performance or awkward lifetimes do not justify unchecked path/string/lifetime assumptions |
| [`non_static_anymap.md`](https://github.com/Nekrolm/crabbook/blob/master/non_static_anymap.md) | Not currently applicable | There is no type-erased heterogeneous state store; do not introduce one for ordinary TUI state |
| [`send_and_sync.md`](https://github.com/Nekrolm/crabbook/blob/master/send_and_sync.md) | High | Moving a single backend owner requires `Send`; shared concurrent access and `Sync` are unnecessary |
| [`consume_and_borrowing.md`](https://github.com/Nekrolm/crabbook/blob/master/consume_and_borrowing.md) | High | Stream and mutate in place for large data; avoid whole-file materialization and value shuffling on hot paths |
| [`pin.md`](https://github.com/Nekrolm/crabbook/blob/master/pin.md) | Not directly applicable | `Pin` should remain an implementation detail of async/library APIs, not a solution to application state ownership |
| [`you_dont_want_drop.md`](https://github.com/Nekrolm/crabbook/blob/master/you_dont_want_drop.md) | High | Prefer focused guards and explicit commit/cancel transitions over custom `Drop` on large stateful types |
| [`event_loops_and_shared_state.md`](https://github.com/Nekrolm/crabbook/blob/master/event_loops_and_shared_state.md) | Very high | UI state belongs to the event loop; device state belongs to one worker; communicate by commands/events |
| [`fn_traits_in_structs.md`](https://github.com/Nekrolm/crabbook/blob/master/fn_traits_in_structs.md) | Low | Prefer explicit command/event enums to storing closure types throughout public/state structs |
| [`impl_trait_compilation_blow.md`](https://github.com/Nekrolm/crabbook/blob/master/impl_trait_compilation_blow.md) | Medium | The `dyn DeviceBackend` boundary is reasonable; avoid recursive generic/reference layering without evidence |
| [`dangerous_variance.md`](https://github.com/Nekrolm/crabbook/blob/master/dangerous_variance.md) | Not currently applicable | There are no custom lifetime-erased containers or unsafe variance claims; keep it that way |

## Definition of done

The highest-priority work is complete only when:

- A device name cannot select a host path outside the chosen directory.
- Failed/cancelled pulls preserve an existing destination and clean temporary artifacts whenever normal process execution permits.
- Upload memory is bounded independently of file size, and a persistent read error terminates rather than producing an unbounded iterator.
- Overwrite messaging matches the guarantee the device can actually provide.
- No MTP call blocks the render/input thread.
- Shutdown behavior during every in-flight state is explicit and tested.
- Errors are either surfaced or deliberately represented as warnings; they are not silently converted to missing entries/default settings.
- Automated fault tests pass, Clippy remains warning-free, and real-device smoke tests pass on each supported OS/device class before a correctness claim is made.
