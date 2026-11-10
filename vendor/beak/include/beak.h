/* Beak's C ABI, written from the Rust crate beak-c by cbindgen: the reference is the comments on each call. */

#ifndef BEAK_H
#define BEAK_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

// How a call came out; the reason a call that failed gave is read with `beak_last_error`.
typedef enum {
  BEAK_STATUS_OK = 0,
  // Nothing has the name, or no such object is there.
  BEAK_STATUS_NOT_FOUND,
  // A transition that may not happen now: the reason says which.
  BEAK_STATUS_REFUSED,
  // An argument that names nothing the call takes.
  BEAK_STATUS_INVALID,
  // A name is there already.
  BEAK_STATUS_EXISTS,
  // What it reaches takes no change.
  BEAK_STATUS_READ_ONLY,
  // The access is denied.
  BEAK_STATUS_DENIED,
  // The system has no such thing.
  BEAK_STATUS_UNSUPPORTED,
  // No program of the name on the machine.
  BEAK_STATUS_MISSING_PROGRAM,
  // The input, or the object, is closed.
  BEAK_STATUS_CLOSED,
  // What is beneath failed: a device, the system.
  BEAK_STATUS_FAILED,
  // Beak itself failed, a fault of its own: the reason says where.
  BEAK_STATUS_PANICKED,
} beak_status;

// What a command runs: a program and its arguments, or a command line the machine's shell runs.
typedef enum {
  BEAK_PROGRAM_KIND_ARGV,
  BEAK_PROGRAM_KIND_SHELL,
} beak_program_kind;

// How a command's streams are wired: through pipes, its output and errors one stream kept and told; as a coprocess,
// its output told and its errors kept; on a channel of descriptors 3 and 4, its own streams kept; or on a terminal of
// its own, `rows` by `cols`, keeping `scrollback_lines` of what scrolls off it.
typedef enum {
  BEAK_STDIO_PIPE,
  BEAK_STDIO_COPROCESS,
  BEAK_STDIO_CHANNEL,
  BEAK_STDIO_TERMINAL,
} beak_stdio;

// A signal, as `beak-signal` names them.
typedef enum {
  BEAK_SIGNAL_HANGUP,
  BEAK_SIGNAL_INTERRUPT,
  BEAK_SIGNAL_TERMINATE,
  BEAK_SIGNAL_KILL,
  BEAK_SIGNAL_STOP,
  BEAK_SIGNAL_CONTINUE,
} beak_signal;

// Where a selector looks.
typedef enum {
  // Directly in the directory, keyed from `from` on where it has one.
  BEAK_SCOPE_IN,
  // Anywhere under the directory.
  BEAK_SCOPE_UNDER,
} beak_scope;

// How the directories of a label are laid out: those of one value, each named by its key; each value a directory of
// those carrying it; or every one carrying it, whatever its value.
typedef enum {
  BEAK_LABEL_VIEW_ONE_VALUE,
  BEAK_LABEL_VIEW_BY_VALUE,
  BEAK_LABEL_VIEW_ANY_VALUE,
} beak_label_view;

// What a name in a namespace is.
typedef enum {
  BEAK_FILE_KIND_DIRECTORY,
  BEAK_FILE_KIND_FILE,
  BEAK_FILE_KIND_LINK,
} beak_file_kind;

// What an entry names.
typedef enum {
  BEAK_KIND_DIRECTORY,
  BEAK_KIND_FILE,
} beak_kind;

// Where a process is in its life.
typedef enum {
  BEAK_PROCESS_STATE_RUNNABLE,
  BEAK_PROCESS_STATE_RUNNING,
  BEAK_PROCESS_STATE_SLEEPING,
  BEAK_PROCESS_STATE_EXITING,
  BEAK_PROCESS_STATE_ZOMBIE,
} beak_process_state;

// How a process ended.
typedef enum {
  // It has not.
  BEAK_OUTCOME_STATUS_NONE,
  BEAK_OUTCOME_STATUS_COMPLETED,
  BEAK_OUTCOME_STATUS_FAILED,
  BEAK_OUTCOME_STATUS_STOPPED,
} beak_outcome_status;

// Why a process that would run is held.
typedef enum {
  BEAK_HELD_NO,
  BEAK_HELD_RESTART_LIMIT,
  BEAK_HELD_MISSING_PROGRAM,
  BEAK_HELD_UNKNOWN_VERSION,
} beak_held;

// A command.
typedef struct beak_command beak_command;

// A kernel.
typedef struct beak_kernel beak_kernel;

// A namespace.
typedef struct beak_namespace beak_namespace;

// A completion queue.
typedef struct beak_queue beak_queue;

// A snapshot.
typedef struct beak_snapshot beak_snapshot;

// Bytes Beak gives back, the caller's to let go of with `beak_buffer_free`.
typedef struct {
  uint8_t *ptr;
  size_t len;
} beak_buffer;

// Bytes a call is given, borrowed for the call alone.
typedef struct {
  const uint8_t *ptr;
  size_t len;
} beak_bytes;

// A name and its value.
typedef struct {
  beak_bytes name;
  beak_bytes value;
} beak_pair;

// A command to start: its program, in the absolute directory `cwd`; its variables, each of `set` over the machine's,
// or those alone where `clear`; its streams; the first `head_bytes` and newest `tail_bytes` of what it keeps; and a
// log of all of it at `log_path`, up to `log_bytes_max`, where the path is not empty.
typedef struct {
  beak_program_kind program;
  const beak_bytes *argv;
  size_t argc;
  beak_bytes line;
  beak_bytes cwd;
  const beak_pair *set;
  size_t set_count;
  bool clear;
  beak_stdio stdio;
  uint16_t rows;
  uint16_t cols;
  size_t scrollback_lines;
  size_t head_bytes;
  size_t tail_bytes;
  beak_bytes log_path;
  uint64_t log_bytes_max;
} beak_spec;

// What a follower reads on from a cursor: the bytes held from `from` on without a break, `from` past the cursor where
// what lay between is no longer held.
typedef struct {
  uint64_t from;
  beak_buffer bytes;
} beak_since;

// How a kernel opens: the directory its volume is kept in — memory, gone with it, where it is empty — and the time.
typedef struct {
  beak_bytes directory;
  int64_t time;
} beak_kernel_options;

// What runs the processes of a kind: the version they run, whether a stop handler runs once one is asked to stop, and
// how a phase that throws runs again — `retries` times, waiting twice as long each time from the least to the most.
typedef struct {
  uint32_t version;
  bool stop_handler;
  uint32_t retries;
  int64_t retry_delay_min_ms;
  int64_t retry_delay_max_ms;
} beak_program;

// A number a call is given, or none.
typedef struct {
  bool has;
  int64_t value;
} beak_maybe_i64;

// Bytes a call is given, or none.
typedef struct {
  bool has;
  beak_bytes bytes;
} beak_maybe_bytes;

// A label or an annotation set to its value, or removed where it has none.
typedef struct {
  beak_bytes name;
  beak_maybe_bytes value;
} beak_name_change;

// Mail, or a request: its key, its name and the host's payload.
typedef struct {
  beak_bytes key;
  beak_bytes name;
  beak_bytes payload;
} beak_mail;

// A count a call is given, or none.
typedef struct {
  bool has;
  uint64_t value;
} beak_maybe_u64;

// A counter charged: its name and what was used.
typedef struct {
  beak_bytes name;
  uint64_t value;
} beak_counter;

// A change a commit makes.
enum beak_change_Tag
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
  // The directory made, its parent there; one there is left as it is.
  BEAK_CHANGE_MKDIR,
  // The file given `bytes` whole, made where there is none, its mode kept.
  BEAK_CHANGE_WRITE,
  // `bytes` written over the file from `offset`, as pwrite writes them.
  BEAK_CHANGE_WRITE_AT,
  // `bytes` added at the file's end, made where there is none; `append_only`, it takes bytes at its end alone after.
  BEAK_CHANGE_APPEND,
  // The file cut or grown to `length`.
  BEAK_CHANGE_TRUNCATE,
  // The file made executable or not.
  BEAK_CHANGE_CHMOD,
  // The file sealed: it takes no change from then on.
  BEAK_CHANGE_SEAL,
  // What `from` names given the name `to`.
  BEAK_CHANGE_RENAME,
  // The name deleted, with everything under it.
  BEAK_CHANGE_DELETE,
  // The name goes at `at`; never, where it has none.
  BEAK_CHANGE_EXPIRE,
  // The directory's labels and annotations set and removed.
  BEAK_CHANGE_META,
  // A process of `kind` and `version` started under `parent`, keyed by `key`, from `input` and `checkpoint`; in the
  // namespace `namespace` names, the nearest process above it's where it has none.
  BEAK_CHANGE_START,
  // A request the process makes of the host.
  BEAK_CHANGE_REQUEST,
  // Mail sent to the process.
  BEAK_CHANGE_SEND,
  // The request of the mail's key answered by it: refused where none waits.
  BEAK_CHANGE_ANSWER,
  // The process, and every one under it and what they started on the machine, sent `signal` for `reason`.
  BEAK_CHANGE_SIGNAL,
  // Counters charged to the process and every group above it.
  BEAK_CHANGE_CHARGE,
  // A held process reset, so it runs again.
  BEAK_CHANGE_RETRY,
  // An ended process's record let go of.
  BEAK_CHANGE_REAP,
  // A name removed, the processes under it asked to stop first.
  BEAK_CHANGE_REMOVE,
  // A name removed, the processes under it killed first.
  BEAK_CHANGE_REMOVE_KILLING,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum beak_change_Tag beak_change_Tag;
#else
typedef uint8_t beak_change_Tag;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

typedef struct {
  beak_bytes path;
} beak_change_Mkdir_Body;

typedef struct {
  beak_bytes path;
  beak_bytes bytes;
} beak_change_Write_Body;

typedef struct {
  beak_bytes path;
  uint64_t offset;
  beak_bytes bytes;
} beak_change_WriteAt_Body;

typedef struct {
  beak_bytes path;
  beak_bytes bytes;
  bool append_only;
} beak_change_Append_Body;

typedef struct {
  beak_bytes path;
  uint64_t length;
} beak_change_Truncate_Body;

typedef struct {
  beak_bytes path;
  bool executable;
} beak_change_Chmod_Body;

typedef struct {
  beak_bytes path;
} beak_change_Seal_Body;

typedef struct {
  beak_bytes from;
  beak_bytes to;
} beak_change_Rename_Body;

typedef struct {
  beak_bytes path;
} beak_change_Delete_Body;

typedef struct {
  beak_bytes path;
  beak_maybe_i64 at;
} beak_change_Expire_Body;

typedef struct {
  beak_bytes path;
  const beak_name_change *labels;
  size_t label_count;
  const beak_name_change *annotations;
  size_t annotation_count;
} beak_change_Meta_Body;

typedef struct {
  beak_bytes parent;
  beak_bytes key;
  beak_bytes kind;
  uint32_t version;
  beak_bytes input;
  beak_bytes checkpoint;
  uint8_t priority;
  beak_maybe_bytes namespace_;
} beak_change_Start_Body;

typedef struct {
  beak_bytes process;
  beak_mail mail;
  beak_maybe_u64 started;
} beak_change_Request_Body;

typedef struct {
  beak_bytes process;
  beak_mail mail;
  beak_maybe_u64 started;
} beak_change_Send_Body;

typedef struct {
  beak_bytes process;
  beak_mail mail;
  beak_maybe_u64 started;
} beak_change_Answer_Body;

typedef struct {
  beak_bytes process;
  beak_signal signal;
  beak_maybe_bytes reason;
  beak_maybe_u64 started;
} beak_change_Signal_Body;

typedef struct {
  beak_bytes process;
  const beak_counter *counters;
  size_t count;
} beak_change_Charge_Body;

typedef struct {
  beak_bytes process;
} beak_change_Retry_Body;

typedef struct {
  beak_bytes process;
} beak_change_Reap_Body;

typedef struct {
  beak_bytes path;
  beak_maybe_bytes reason;
} beak_change_Remove_Body;

typedef struct {
  beak_bytes path;
  beak_maybe_bytes reason;
} beak_change_RemoveKilling_Body;

typedef struct {
  beak_change_Tag tag;
  union {
    beak_change_Mkdir_Body MKDIR;
    beak_change_Write_Body WRITE;
    beak_change_WriteAt_Body WRITE_AT;
    beak_change_Append_Body APPEND;
    beak_change_Truncate_Body TRUNCATE;
    beak_change_Chmod_Body CHMOD;
    beak_change_Seal_Body SEAL;
    beak_change_Rename_Body RENAME;
    beak_change_Delete_Body DELETE;
    beak_change_Expire_Body EXPIRE;
    beak_change_Meta_Body META;
    beak_change_Start_Body START;
    beak_change_Request_Body REQUEST;
    beak_change_Send_Body SEND;
    beak_change_Answer_Body ANSWER;
    beak_change_Signal_Body SIGNAL;
    beak_change_Charge_Body CHARGE;
    beak_change_Retry_Body RETRY;
    beak_change_Reap_Body REAP;
    beak_change_Remove_Body REMOVE;
    beak_change_RemoveKilling_Body REMOVE_KILLING;
  };
} beak_change;

// A run for the host to start: the phase of the process at `path`, the one the commit `started` started — or its stop
// handler — from its input and the checkpoint it runs from.
typedef struct {
  beak_buffer path;
  uint64_t started;
  bool stop_handler;
  beak_buffer kind;
  uint32_t version;
  beak_buffer input;
  beak_buffer checkpoint;
} beak_run;

// The runs a step reserved.
typedef struct {
  beak_run *items;
  size_t len;
} beak_runs;

// The run a host finishes: the process's path, the commit that started it, whether it was the stop handler, its kind.
typedef struct {
  beak_bytes path;
  uint64_t started;
  bool stop_handler;
  beak_bytes kind;
} beak_run_name;

// A directory a wait watches, with the commit that last changed it as the wait began; none for one not there.
typedef struct {
  beak_bytes path;
  beak_maybe_u64 changed_seq;
} beak_version;

// The directories a selector picks: of its kinds, carrying its labels, where it looks.
typedef struct {
  const beak_bytes *kinds;
  size_t kind_count;
  const beak_pair *labels;
  size_t label_count;
  beak_scope scope;
  beak_bytes directory;
  beak_maybe_bytes from;
} beak_selector;

// What a process waits for, the first met ending the wait: the processes under it, by their keys, every one ending;
// directories changing; a directory the selector picks — `until`, or null; the time `at`.
typedef struct {
  const beak_bytes *on;
  size_t on_count;
  const beak_version *changes;
  size_t change_count;
  const beak_selector *until;
  beak_maybe_i64 at;
} beak_wait;

// How a phase, or a stop handler, ended.
enum beak_ending_Tag
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
  // Runnable again, its next phase from the checkpoint.
  BEAK_ENDING_NEXT,
  // It sleeps on the wait, then runs from the checkpoint.
  BEAK_ENDING_WAIT,
  // It completes with the result.
  BEAK_ENDING_DONE,
  // It fails with the error.
  BEAK_ENDING_FAIL,
  // The phase threw: it runs again while its retries last, then fails.
  BEAK_ENDING_THREW,
  // The stop handler returned: it ends stopped, with the result where it has one.
  BEAK_ENDING_STOPPED,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum beak_ending_Tag beak_ending_Tag;
#else
typedef uint8_t beak_ending_Tag;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

typedef struct {
  beak_bytes checkpoint;
} beak_ending_Next_Body;

typedef struct {
  beak_bytes checkpoint;
  beak_wait wait;
} beak_ending_Wait_Body;

typedef struct {
  beak_bytes result;
} beak_ending_Done_Body;

typedef struct {
  beak_bytes error;
} beak_ending_Fail_Body;

typedef struct {
  beak_bytes message;
} beak_ending_Threw_Body;

typedef struct {
  beak_maybe_bytes result;
} beak_ending_Stopped_Body;

typedef struct {
  beak_ending_Tag tag;
  union {
    beak_ending_Next_Body NEXT;
    beak_ending_Wait_Body WAIT;
    beak_ending_Done_Body DONE;
    beak_ending_Fail_Body FAIL;
    beak_ending_Threw_Body THREW;
    beak_ending_Stopped_Body STOPPED;
  };
} beak_ending;

// An id, as its 36 characters of text.
typedef struct {
  uint8_t text[36];
} beak_id;

// What a collection did: the objects it kept and those it forgot.
typedef struct {
  uint64_t kept;
  uint64_t forgotten;
} beak_collected;

// A file system to mount.
enum beak_file_system_Tag
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
  // The machine's directory at the absolute `path`.
  BEAK_FILE_SYSTEM_HOST,
  // A place nothing reaches.
  BEAK_FILE_SYSTEM_INACCESSIBLE,
  // The directories of `kind` under `under` carrying the label `label`, of `value` for a view of one value, laid out
  // as `view` says, read alone.
  BEAK_FILE_SYSTEM_LABEL,
  // The kernel's volume from `base` down, read alone.
  BEAK_FILE_SYSTEM_TREE,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum beak_file_system_Tag beak_file_system_Tag;
#else
typedef uint8_t beak_file_system_Tag;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

typedef struct {
  beak_bytes path;
} beak_file_system_Host_Body;

typedef struct {
  const beak_kernel *kernel;
  beak_bytes kind;
  beak_bytes label;
  beak_label_view view;
  beak_bytes value;
  beak_bytes under;
} beak_file_system_Label_Body;

typedef struct {
  const beak_kernel *kernel;
  beak_bytes base;
} beak_file_system_Tree_Body;

typedef struct {
  beak_file_system_Tag tag;
  union {
    beak_file_system_Host_Body HOST;
    beak_file_system_Label_Body LABEL;
    beak_file_system_Tree_Body TREE;
  };
} beak_file_system;

// What a name is: its kind, a file's length, whether it is executable, when it last changed — 0 where its file system
// keeps no time — and the number its file system knows it by, where it has one.
typedef struct {
  beak_file_kind kind;
  uint64_t size;
  bool executable;
  int64_t modified_ms;
  bool has_file_id;
  uint64_t file_id;
} beak_stat;

// A name in a directory, and what it names.
typedef struct {
  beak_buffer name;
  beak_file_kind kind;
} beak_dirent;

// Names in a directory, Beak's.
typedef struct {
  beak_dirent *items;
  size_t len;
} beak_dirents;

// Which entries of a directory a page reads: those whose names start with `prefix`, past `after` in the direction read,
// `limit` at most.
typedef struct {
  beak_bytes prefix;
  beak_maybe_bytes after;
  bool reverse;
  size_t limit;
} beak_range;

// How a command ended: by a signal, its number; else the code it exited with.
typedef struct {
  bool signalled;
  int32_t value;
} beak_exit;

// What happened.
enum beak_event_Tag
#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
  : uint8_t
#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
 {
  // A commit was made: what it changed, the history reads on to.
  BEAK_EVENT_COMMITTED,
  // A request a process made, for its handler to act on: `retold` where it was made before the volume opened, and
  // the host told of it first may have acted on it.
  BEAK_EVENT_REQUEST,
  // A request cut off, its process asked to stop or interrupted: the host answers it now, with what it has.
  BEAK_EVENT_INTERRUPTED,
  // A group a charge took past its limit.
  BEAK_EVENT_LIMIT,
  // A process, and every one under it, sent a signal.
  BEAK_EVENT_SIGNALLED,
  // What a command wrote of the stream it tells.
  BEAK_EVENT_OUTPUT,
  // The title the program on a command's terminal set.
  BEAK_EVENT_TITLE,
  // A command's end, told last.
  BEAK_EVENT_EXIT,
};
#ifndef __cplusplus
#if __STDC_VERSION__ >= 202311L
typedef enum beak_event_Tag beak_event_Tag;
#else
typedef uint8_t beak_event_Tag;
#endif // __STDC_VERSION__ >= 202311L
#endif // __cplusplus

typedef struct {
  uint64_t seq;
} beak_event_Committed_Body;

typedef struct {
  beak_buffer process;
  beak_buffer key;
  beak_buffer name;
  beak_buffer payload;
  uint64_t started;
  bool retold;
} beak_event_Request_Body;

typedef struct {
  beak_buffer process;
  beak_buffer key;
} beak_event_Interrupted_Body;

typedef struct {
  beak_buffer process;
  beak_buffer name;
} beak_event_Limit_Body;

typedef struct {
  beak_buffer process;
  beak_signal signal;
} beak_event_Signalled_Body;

typedef struct {
  beak_buffer bytes;
} beak_event_Output_Body;

typedef struct {
  beak_buffer title;
} beak_event_Title_Body;

typedef struct {
  beak_exit exit;
} beak_event_Exit_Body;

typedef struct {
  beak_event_Tag tag;
  union {
    beak_event_Committed_Body COMMITTED;
    beak_event_Request_Body REQUEST;
    beak_event_Interrupted_Body INTERRUPTED;
    beak_event_Limit_Body LIMIT;
    beak_event_Signalled_Body SIGNALLED;
    beak_event_Output_Body OUTPUT;
    beak_event_Title_Body TITLE;
    beak_event_Exit_Body EXIT;
  };
} beak_event;

// A completion: what happened, and the tag the host gave what it is of.
typedef struct {
  uint64_t tag;
  beak_event event;
} beak_completion;

// A name and its value, Beak's.
typedef struct {
  beak_buffer name;
  beak_buffer value;
} beak_named;

// Names and their values, Beak's.
typedef struct {
  beak_named *items;
  size_t len;
} beak_names;

// A directory: its labels, which the label index finds it by; its annotations, which it does not; the commit that made
// it and the last that changed it.
typedef struct {
  beak_names labels;
  beak_names annotations;
  uint64_t created_seq;
  uint64_t changed_seq;
} beak_directory;

// An entry: its name, what it names, a file's length and mode, when it goes, what it holds — its bytes and objects,
// everything under it included — and a directory's own, null for a file.
typedef struct {
  beak_buffer name;
  beak_kind kind;
  uint64_t length;
  bool executable;
  bool append_only;
  bool sealed;
  beak_maybe_i64 expires_at;
  uint64_t size_bytes;
  uint64_t size_objects;
  beak_directory *directory;
} beak_entry;

// Entries, Beak's.
typedef struct {
  beak_entry *items;
  size_t len;
} beak_entries;

// A directory the label index found: its path, its label's value, and its entry.
typedef struct {
  beak_buffer path;
  beak_buffer value;
  beak_entry entry;
} beak_found;

// What the label index found, Beak's.
typedef struct {
  beak_found *items;
  size_t len;
} beak_found_list;

// What the label index is asked: the directories of `kind` carrying the label `name` — of `value`, or of any — under
// `under`, anywhere for the root, past the path `after`.
typedef struct {
  beak_bytes kind;
  beak_bytes name;
  beak_maybe_bytes value;
  beak_bytes under;
  beak_maybe_bytes after;
} beak_query;

// Bytes Beak gives back, or none.
typedef struct {
  bool has;
  beak_buffer buffer;
} beak_maybe_buffer;

// A counter of what a group used, and its value.
typedef struct {
  beak_buffer name;
  uint64_t value;
} beak_used;

// What a group used, by counter.
typedef struct {
  beak_used *items;
  size_t len;
} beak_usage;

// A process's record: its program, the commit that started it, where it is in its life, its wait's time, how it
// ended, the stop asked of it, why it is held, its counts, the namespace its commands start in, what its group used,
// and when it started, its run did and it ended.
typedef struct {
  beak_buffer kind;
  uint32_t version;
  uint64_t created_seq;
  beak_process_state state;
  bool frozen;
  uint8_t priority;
  beak_maybe_buffer checkpoint;
  beak_maybe_i64 waits_until;
  beak_outcome_status outcome;
  beak_maybe_buffer result;
  beak_maybe_buffer error;
  beak_maybe_buffer reason;
  bool terminating;
  beak_maybe_buffer term_reason;
  int64_t term_at;
  beak_held held;
  uint32_t restarts;
  uint32_t attempt;
  beak_maybe_buffer namespace_;
  beak_usage usage;
  beak_maybe_i64 started_at;
  beak_maybe_i64 run_since;
  beak_maybe_i64 ended_at;
} beak_record;

// A change a commit made: a name deleted, with everything under it; or written, a directory with its labels, and its
// labels before where the commit changed them.
typedef struct {
  bool deleted;
  beak_buffer path;
  beak_kind kind;
  beak_names labels;
  bool had_labels;
  beak_names labels_before;
} beak_changed;

// What a commit changed: its sequence number, its time, and its changes.
typedef struct {
  uint64_t seq;
  int64_t time;
  beak_changed *changes;
  size_t change_count;
} beak_change_set;

// The history after a commit: the change sets read, each with what the filter met; the last commit read, which a
// reader reads on from; the oldest the journal keeps; the snapshot's; and whether the journal no longer holds every
// commit since, so the reader looks at everything again.
typedef struct {
  beak_change_set *change_sets;
  size_t change_set_count;
  uint64_t through;
  beak_maybe_u64 oldest;
  uint64_t latest;
  bool missed;
} beak_history;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// The reason the last call on this thread that failed gave, as far as `capacity` bytes of `out` hold it, not ended by a
// NUL: how many bytes the whole reason takes, so a larger buffer can be asked for.
//
// # Safety
// `out` is null, or `capacity` bytes writable.
size_t beak_last_error(uint8_t *out,
                       size_t capacity);

// Lets go of the bytes of a buffer Beak gave, which is empty after.
//
// # Safety
// `buffer` is null, or a buffer Beak gave and nothing let go of.
void beak_buffer_free(beak_buffer *buffer);

// Starts the command `spec` describes at `time` in `namespace`, recorded under the directory `under` of the kernel's
// volume — taking the signals sent to the subtree it lies in, killed as its record goes — what it tells told on
// `queue` with `tag`.
//
// # Safety
// `kernel`, `namespace`, `spec` and `queue` are valid; `under` is valid for the call; `out` is writable.
beak_status beak_kernel_spawn(const beak_kernel *kernel,
                              int64_t time,
                              beak_bytes under,
                              const beak_namespace *namespace_,
                              const beak_spec *spec,
                              const beak_queue *queue,
                              uint64_t tag,
                              beak_command **out);

// Starts the command `spec` describes at `time` for the process at `process`, in the namespace it names, recorded under
// it; what it tells told on `queue` with `tag`. `NotFound` where no process is there, or it names no namespace the
// kernel registered.
//
// # Safety
// `kernel`, `spec` and `queue` are valid; `process` is valid for the call; `out` is writable.
beak_status beak_kernel_spawn_for(const beak_kernel *kernel,
                                  int64_t time,
                                  beak_bytes process,
                                  const beak_spec *spec,
                                  const beak_queue *queue,
                                  uint64_t tag,
                                  beak_command **out);

// Lets go of a command: one still running is hung up on, or its input closed, and killed past a grace, nobody
// waiting; its end is still told.
//
// # Safety
// `command` is null, or one Beak gave and nothing let go of.
void beak_command_free(beak_command *command);

// The id of a command's group.
//
// # Safety
// `command` is valid.
uint32_t beak_command_group(const beak_command *command);

// Takes `bytes` to write to the command's input — what is typed, for one on a terminal — written in order as it reads,
// the call never waiting. `Closed` where its input was closed, or it has none.
//
// # Safety
// `command` is valid; `bytes` is valid for the call.
beak_status beak_command_write(const beak_command *command,
                               beak_bytes bytes);

// Closes the command's input once what was taken is written, as the end of a file.
//
// # Safety
// `command` is valid.
beak_status beak_command_close_input(const beak_command *command);

// Sends `signal` to the command's group; one that ended takes none.
//
// # Safety
// `command` is valid.
beak_status beak_command_signal(const beak_command *command, beak_signal signal);

// Sizes the command's terminal to `rows` by `cols`, its program told. `Invalid` for a command with no terminal.
//
// # Safety
// `command` is valid.
beak_status beak_command_resize(const beak_command *command,
                                uint16_t rows,
                                uint16_t cols);

// What the command keeps, as a reader is shown it: its head, its newest tail, and between them how many bytes were
// left out and what its log keeps of them.
//
// # Safety
// `command` is valid; `out` is writable.
beak_status beak_command_output(const beak_command *command,
                                beak_buffer *out);

// What the command keeps from `cursor` on, as a follower reads on.
//
// # Safety
// `command` is valid; `out` is writable.
beak_status beak_command_since(const beak_command *command, uint64_t cursor, beak_since *out);

// The sequences that draw the command's terminal again whole. `Invalid` for a command with no terminal.
//
// # Safety
// `command` is valid; `out` is writable.
beak_status beak_command_screen(const beak_command *command,
                                beak_buffer *out);

// What the command's terminal shows, as text. `Invalid` for a command with no terminal.
//
// # Safety
// `command` is valid; `out` is writable.
beak_status beak_command_screen_text(const beak_command *command, beak_buffer *out);

// The title the program on the command's terminal set. `Invalid` for a command with no terminal.
//
// # Safety
// `command` is valid; `out` is writable.
beak_status beak_command_title(const beak_command *command, beak_buffer *out);

// Opens a kernel as `options` say, its events told on `queue` with `tag`: first every request still waiting, retold,
// as the commands a crashed host left are stopped.
//
// # Safety
// `options` and `queue` are valid; `out` is writable.
beak_status beak_kernel_open(const beak_kernel_options *options,
                             const beak_queue *queue,
                             uint64_t tag,
                             beak_kernel **out);

// Closes a kernel: the commands it started are hung up on, its volume closed.
//
// # Safety
// `kernel` is null, or one `beak_kernel_open` gave and nothing let go of.
void beak_kernel_free(beak_kernel *kernel);

// Registers the program of `kind`; the latest registration wins.
//
// # Safety
// `kernel` and `program` are valid; `kind` is valid for the call.
beak_status beak_kernel_register(const beak_kernel *kernel,
                                 beak_bytes kind,
                                 const beak_program *program);

// Commits `changes` at `time`, all or none: its sequence number in `seq`.
//
// # Safety
// `kernel` is valid; `changes` holds `count` changes valid for the call; `seq` is writable.
beak_status beak_kernel_commit(const beak_kernel *kernel,
                               int64_t time,
                               const beak_change *changes,
                               size_t count,
                               uint64_t *seq);

// Lets go of the runs a step gave.
//
// # Safety
// `runs` is null, or what `beak_kernel_runnable` gave and nothing let go of.
void beak_runs_free(beak_runs *runs);

// The runs to start at `time`, in one commit: what exited ended, what outlived its grace killed, what cannot run held,
// what can reserved within the bounds.
//
// # Safety
// `kernel` is valid; `out` is writable.
beak_status beak_kernel_runnable(const beak_kernel *kernel,
                                 int64_t time,
                                 beak_runs **out);

// Finishes `run` at `time`: `changes` and its `ending` in one commit, its sequence number in `seq`. Where a change
// cannot apply, the run ends as thrown, nothing of the changes made. A run of a process started again since is
// refused `stale`, one not running `not-admitted`.
//
// # Safety
// `kernel`, `run` and `ending` are valid; `changes` holds `count` changes valid for the call; `seq` is writable.
beak_status beak_kernel_finish(const beak_kernel *kernel,
                               int64_t time,
                               const beak_run_name *run,
                               const beak_ending *ending,
                               const beak_change *changes,
                               size_t count,
                               uint64_t *seq);

// When the kernel is next due — a wait's time, a grace running out, a name's time — for the host's alarm: none for
// never.
//
// # Safety
// `kernel` is valid; `out` is writable.
beak_status beak_kernel_next_due(const beak_kernel *kernel,
                                 beak_maybe_i64 *out);

// A new id minted at `time`, later than every one the volume minted before.
//
// # Safety
// `kernel` is valid; `out` is writable.
beak_status beak_kernel_mint(const beak_kernel *kernel, int64_t time, beak_id *out);

// Makes every commit so far survive a power loss.
//
// # Safety
// `kernel` is valid.
beak_status beak_kernel_sync(const beak_kernel *kernel);

// The bytes the volume keeps on its device, live or not.
//
// # Safety
// `kernel` is valid; `out` is writable.
beak_status beak_kernel_usage(const beak_kernel *kernel, uint64_t *out);

// Gives back what no snapshot still held reaches.
//
// # Safety
// `kernel` is valid; `out` is writable.
beak_status beak_kernel_collect(const beak_kernel *kernel, beak_collected *out);

// Opens a namespace of no mounts, reaching the network.
//
// # Safety
// `out` is writable.
beak_status beak_namespace_open(beak_namespace **out);

// Lets go of a namespace; a command started in it, or the kernel that registered it, keeps what it mounts.
//
// # Safety
// `namespace` is null, or one Beak gave and nothing let go of.
void beak_namespace_free(beak_namespace *namespace_);

// Mounts `file_system` at `at`, in place of what was mounted there, written through where `writable`; "" is the root.
//
// # Safety
// `namespace` and `file_system` are valid; `at` is valid for the call.
beak_status beak_namespace_mount(const beak_namespace *namespace_,
                                 beak_bytes at,
                                 const beak_file_system *file_system,
                                 bool writable);

// Takes out what was mounted at `at`; `NotFound` where nothing was.
//
// # Safety
// `namespace` is valid; `at` is valid for the call.
beak_status beak_namespace_unmount(const beak_namespace *namespace_, beak_bytes at);

// Whether what works in the namespace reaches the network.
//
// # Safety
// `namespace` is valid.
beak_status beak_namespace_reach_network(const beak_namespace *namespace_, bool reaches);

// Registers a copy of `namespace` with the kernel as `name`: the commands of a process naming it start in it.
//
// # Safety
// `kernel` and `namespace` are valid; `name` is valid for the call.
beak_status beak_kernel_register_namespace(const beak_kernel *kernel,
                                           beak_bytes name,
                                           const beak_namespace *namespace_);

// Lets go of the namespace the kernel registered as `name`.
//
// # Safety
// `kernel` is valid; `name` is valid for the call.
beak_status beak_kernel_forget_namespace(const beak_kernel *kernel, beak_bytes name);

// What `path` names. `NotFound` where nothing does.
//
// # Safety
// `namespace` is valid; `path` is valid for the call; `out` is writable.
beak_status beak_namespace_stat(const beak_namespace *namespace_, beak_bytes path, beak_stat *out);

// The bytes of the file at `path` from `offset`, at most `length`.
//
// # Safety
// `namespace` is valid; `path` is valid for the call; `out` is writable.
beak_status beak_namespace_read(const beak_namespace *namespace_,
                                beak_bytes path,
                                uint64_t offset,
                                uint64_t length,
                                beak_buffer *out);

// The file at `path` given `bytes` whole, made where there is none, its mode kept.
//
// # Safety
// `namespace` is valid; `path` and `bytes` are valid for the call.
beak_status beak_namespace_write(const beak_namespace *namespace_,
                                 beak_bytes path,
                                 beak_bytes bytes);

// `bytes` written over the file at `path` from `offset`, as pwrite writes them.
//
// # Safety
// `namespace` is valid; `path` and `bytes` are valid for the call.
beak_status beak_namespace_write_at(const beak_namespace *namespace_,
                                    beak_bytes path,
                                    uint64_t offset,
                                    beak_bytes bytes);

// The file at `path` cut or grown to `length`.
//
// # Safety
// `namespace` is valid; `path` is valid for the call.
beak_status beak_namespace_truncate(const beak_namespace *namespace_,
                                    beak_bytes path,
                                    uint64_t length);

// The file at `path` made executable or not.
//
// # Safety
// `namespace` is valid; `path` is valid for the call.
beak_status beak_namespace_chmod(const beak_namespace *namespace_,
                                 beak_bytes path,
                                 bool executable);

// The directory at `path` made, and those on the way, as `mkdir -p` makes them.
//
// # Safety
// `namespace` is valid; `path` is valid for the call.
beak_status beak_namespace_mkdir(const beak_namespace *namespace_, beak_bytes path);

// The name at `path` deleted, with everything under it.
//
// # Safety
// `namespace` is valid; `path` is valid for the call.
beak_status beak_namespace_delete(const beak_namespace *namespace_, beak_bytes path);

// What `from` names given the name `to`: within a mount its own rename, between two a copy and a delete.
//
// # Safety
// `namespace` is valid; `from` and `to` are valid for the call.
beak_status beak_namespace_rename(const beak_namespace *namespace_,
                                  beak_bytes from,
                                  beak_bytes to);

// Lets go of names Beak gave.
//
// # Safety
// `dirents` is null, or what Beak gave and nothing let go of.
void beak_dirents_free(beak_dirents *dirents);

// A page of the directory at `path` as `range` picks it, in the order of their names or the reverse, with the
// directories on the way to the mounts under it.
//
// # Safety
// `namespace` and `range` are valid; `path` is valid for the call; `out` is writable.
beak_status beak_namespace_list(const beak_namespace *namespace_,
                                beak_bytes path,
                                const beak_range *range,
                                beak_dirents **out);

// Lets go of what a completion holds.
//
// # Safety
// `completion` is null, or one `beak_queue_take` gave and nothing let go of.
void beak_completion_free(beak_completion *completion);

// Opens a completion queue.
//
// # Safety
// `out` is writable.
beak_status beak_queue_open(beak_queue **out);

// Lets go of a queue; the completions waiting in it go with it, and what is still bound to it tells nothing more.
//
// # Safety
// `queue` is null, or one `beak_queue_open` gave and nothing let go of.
void beak_queue_free(beak_queue *queue);

// The descriptor that reads ready while a completion waits, for an event loop to wait on; -1 where the system has
// none. Read nothing from it: taking the completions empties it.
//
// # Safety
// `queue` is a queue Beak gave.
int32_t beak_queue_fd(const beak_queue *queue);

// Takes at most `max` completions into `out`, oldest first, never waiting; how many in `taken`. Each is the caller's
// to let go of with `beak_completion_free`.
//
// # Safety
// `queue` is a queue Beak gave; `out` has room for `max` completions; `taken` is writable.
beak_status beak_queue_take(const beak_queue *queue,
                            beak_completion *out,
                            size_t max,
                            size_t *taken);

// Waits till a completion waits, `timeout_ms` at most; whether one does in `ready`.
//
// # Safety
// `queue` is a queue Beak gave; `ready` is writable.
beak_status beak_queue_wait(const beak_queue *queue, uint32_t timeout_ms, bool *ready);

// The volume as the last commit left it.
//
// # Safety
// `kernel` is valid; `out` is writable.
beak_status beak_kernel_snapshot(const beak_kernel *kernel, beak_snapshot **out);

// Lets go of a snapshot; what only it held, a collection then gives back.
//
// # Safety
// `snapshot` is null, or one Beak gave and nothing let go of.
void beak_snapshot_free(beak_snapshot *snapshot);

// The commit a snapshot is of; 0 before the first.
//
// # Safety
// `snapshot` is valid.
uint64_t beak_snapshot_seq(const beak_snapshot *snapshot);

// Lets go of an entry Beak gave.
//
// # Safety
// `entry` is null, or one Beak gave and nothing let go of.
void beak_entry_free(beak_entry *entry);

// Lets go of entries Beak gave.
//
// # Safety
// `entries` is null, or what Beak gave and nothing let go of.
void beak_entries_free(beak_entries *entries);

// The entry at `path`, with its directory where it names one; the root at the empty path. `NotFound` where nothing
// has the name.
//
// # Safety
// `snapshot` is valid; `path` is valid for the call; `out` is writable.
beak_status beak_snapshot_stat(const beak_snapshot *snapshot,
                               beak_bytes path,
                               beak_entry **out);

// A page of the directory at `path`, as `range` picks it, in the order of their names or the reverse, each directory
// among them read. `NotFound` where there is no such directory.
//
// # Safety
// `snapshot` and `range` are valid; `path` is valid for the call; `out` is writable.
beak_status beak_snapshot_list(const beak_snapshot *snapshot,
                               beak_bytes path,
                               const beak_range *range,
                               beak_entries **out);

// The bytes of the file at `path` from `offset`, at most `length`. `NotFound` where there is no such name.
//
// # Safety
// `snapshot` is valid; `path` is valid for the call; `out` is writable.
beak_status beak_snapshot_read(const beak_snapshot *snapshot,
                               beak_bytes path,
                               uint64_t offset,
                               uint64_t length,
                               beak_buffer *out);

// Lets go of what the label index found.
//
// # Safety
// `found` is null, or what Beak gave and nothing let go of.
void beak_found_list_free(beak_found_list *found);

// The directories `query` asks for, in the order of their paths, `limit` at most.
//
// # Safety
// `snapshot` and `query` are valid; `out` is writable.
beak_status beak_snapshot_find(const beak_snapshot *snapshot,
                               const beak_query *query,
                               size_t limit,
                               beak_found_list **out);

// Lets go of a record Beak gave.
//
// # Safety
// `record` is null, or one Beak gave and nothing let go of.
void beak_record_free(beak_record *record);

// The record of the process at `path`. `NotFound` where no process is there.
//
// # Safety
// `snapshot` is valid; `path` is valid for the call; `out` is writable.
beak_status beak_snapshot_record(const beak_snapshot *snapshot, beak_bytes path, beak_record **out);

// Lets go of a history Beak gave.
//
// # Safety
// `history` is null, or one Beak gave and nothing let go of.
void beak_history_free(beak_history *history);

// The changes of the commits after `after` up to the snapshot's, `limit` change sets read at most, those one of the
// `selector_count` selectors meets — every one where there are none.
//
// # Safety
// `snapshot` is valid; `selectors` holds `selector_count` selectors valid for the call; `out` is writable.
beak_status beak_snapshot_history(const beak_snapshot *snapshot,
                                  uint64_t after,
                                  size_t limit,
                                  const beak_selector *selectors,
                                  size_t selector_count,
                                  beak_history **out);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* BEAK_H */
