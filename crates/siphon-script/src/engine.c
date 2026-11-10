/*
 * A script on QuickJS: an engine of its own — its runtime and its context — running JavaScript as the body of an async
 * function. The host's functions are under a global object the host names, as Math holds its own, each call a promise
 * the host settles. A step runs the script until it waits on calls, returns, throws or spends its steps; steps are the
 * engine's interrupt ticks, counted, not timed, so a script stops at the same point every time. Its clock tells the
 * time the host gives each step and its random numbers come of the seed it starts with, so a host replaying it with
 * the same times, seed and settlements sees it run alike. What a script came to is kept as it comes to it, and handed
 * to the host through the functions it gives, which copy what they are given; the engine keeps what it holds.
 */
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "quickjs.h"

enum { ENGINE_WAITING = 0, ENGINE_RETURNED = 1, ENGINE_THREW = 2, ENGINE_OUT_OF_STEPS = 3 };

/* Bytes the host gives: not ended by a NUL. */
typedef struct EngineName {
  const char *bytes;
  size_t len;
} EngineName;

/* What the host is handed, to copy. */
typedef void EngineTextFn(void *to, const char *bytes, size_t len);
typedef void EngineCallFn(void *to, uint32_t id, const char *function, size_t function_len, const char *arguments,
                          size_t arguments_len);
typedef void EngineThrownFn(void *to, const char *name, size_t name_len, const char *message, size_t message_len,
                            const char *stack, size_t stack_len);

/* Bytes the engine owns, ended by a NUL. */
typedef struct Text {
  char *bytes;
  size_t len;
} Text;

typedef struct Call {
  uint32_t id;
  uint32_t function;
  Text arguments;
  JSValue resolve;
  JSValue reject;
  int reported;
  struct Call *next;
} Call;

/* A growing buffer of bytes, its memory the runtime's, so it counts against the script's. */
typedef struct Buffer {
  char *bytes;
  size_t len;
  size_t capacity;
} Buffer;

typedef struct Engine {
  JSRuntime *runtime;
  JSContext *context;
  JSValue body;
  char *namespace_name;
  char **functions;
  size_t function_count;
  /* The calls it waits on, oldest first. */
  Call *calls;
  Call *last_call;
  uint32_t next_id;
  uint64_t steps;
  uint64_t steps_max;
  int out_of_steps;
  int state;
  Text returned;
  Text thrown_name;
  Text thrown_message;
  Text thrown_stack;
  /* The lines written since they were last taken: their bytes, and where each ends. */
  Buffer output;
  size_t *line_ends;
  size_t line_count;
  size_t line_capacity;
  int64_t now;
  uint64_t random[4];
} Engine;

static int append(JSRuntime *runtime, Buffer *buffer, const char *bytes, size_t len) {
  if (buffer->len + len + 1 > buffer->capacity) {
    size_t capacity = buffer->capacity ? buffer->capacity * 2 : 256;
    while (capacity < buffer->len + len + 1) capacity *= 2;
    char *grown = js_realloc_rt(runtime, buffer->bytes, capacity);
    if (!grown) return -1;
    buffer->bytes = grown;
    buffer->capacity = capacity;
  }
  memcpy(buffer->bytes + buffer->len, bytes, len);
  buffer->len += len;
  buffer->bytes[buffer->len] = 0;
  return 0;
}

/* A copy of `len` bytes; 0, or -1 with no memory. */
static int text_of(Text *text, const char *bytes, size_t len) {
  free(text->bytes);
  text->bytes = malloc(len + 1);
  text->len = 0;
  if (!text->bytes) return -1;
  memcpy(text->bytes, bytes, len);
  text->bytes[len] = 0;
  text->len = len;
  return 0;
}

static int interrupt(JSContext *context, void *opaque) {
  (void)context;
  Engine *engine = opaque;
  engine->steps += 1;
  if (engine->steps <= engine->steps_max) return 0;
  engine->out_of_steps = 1;
  return 1;
}

/* The next of the seeded numbers, xoshiro256**: the same seed, the same numbers, on every machine. */
static uint64_t next_random(Engine *engine) {
  uint64_t *s = engine->random;
  uint64_t result = ((s[1] * 5) << 7 | (s[1] * 5) >> 57) * 9;
  uint64_t t = s[1] << 17;
  s[2] ^= s[0];
  s[3] ^= s[1];
  s[1] ^= s[2];
  s[0] ^= s[3];
  s[2] ^= t;
  s[3] = s[3] << 45 | s[3] >> 19;
  return result;
}

/* The state xoshiro starts from, made of the seed by splitmix64. */
static void seed_random(Engine *engine, uint64_t seed) {
  for (int i = 0; i < 4; i++) {
    uint64_t z = (seed += 0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9;
    z = (z ^ (z >> 27)) * 0x94d049bb133111eb;
    engine->random[i] = z ^ (z >> 31);
  }
}

static JSValue clock_now(JSContext *context, JSValueConst this_value, int argc, JSValueConst *argv) {
  (void)this_value, (void)argc, (void)argv;
  Engine *engine = JS_GetContextOpaque(context);
  return JS_NewFloat64(context, (double)engine->now);
}

static JSValue clock_random(JSContext *context, JSValueConst this_value, int argc, JSValueConst *argv) {
  (void)this_value, (void)argc, (void)argv;
  Engine *engine = JS_GetContextOpaque(context);
  return JS_NewFloat64(context, (double)(next_random(engine) >> 11) * 0x1.0p-53);
}

/*
 * The clock and the random numbers made the host's: `Date` as a function of the host's time — a new Date of no
 * arguments the time the host gave the step, Date() its text, Date.now() its milliseconds, the rest as Date has them —
 * and Math.random of the seed.
 */
static const char host_clock[] =
    "(function (now, random) {\n"
    "  const Real = globalThis.Date;\n"
    "  function Clock(...args) {\n"
    "    if (!new.target) return new Real(now()).toString();\n"
    "    return Reflect.construct(Real, args.length === 0 ? [now()] : args, new.target);\n"
    "  }\n"
    "  Object.setPrototypeOf(Clock, Real);\n"
    "  Object.defineProperty(Clock, 'prototype', { value: Real.prototype });\n"
    "  Object.defineProperty(Clock, 'name', { value: 'Date' });\n"
    "  Object.defineProperty(Clock, 'length', { value: 7 });\n"
    "  Clock.now = function now_() { return now(); };\n"
    "  Object.defineProperty(Real.prototype, 'constructor', { value: Clock, writable: true, configurable: true });\n"
    "  Object.defineProperty(globalThis, 'Date', { value: Clock, writable: true, configurable: true });\n"
    "  Object.defineProperty(Math, 'random', { value: function random_() { return random(); }, writable: true, configurable: true });\n"
    "})";

static int install_clock(JSContext *context) {
  JSValue made = JS_Eval(context, host_clock, sizeof host_clock - 1, "<clock>", JS_EVAL_TYPE_GLOBAL);
  if (JS_IsException(made)) return -1;
  JSValue arguments[2] = {JS_NewCFunction(context, clock_now, "now", 0), JS_NewCFunction(context, clock_random, "random", 0)};
  JSValue done = JS_Call(context, made, JS_UNDEFINED, 2, arguments);
  JS_FreeValue(context, arguments[0]);
  JS_FreeValue(context, arguments[1]);
  JS_FreeValue(context, made);
  int failed = JS_IsException(done);
  JS_FreeValue(context, done);
  return failed ? -1 : 0;
}

/* A call of the host's function: its argument as JSON, a promise the host settles. */
static JSValue call_function(JSContext *context, JSValueConst this_value, int argc, JSValueConst *argv, int magic) {
  (void)this_value;
  Engine *engine = JS_GetContextOpaque(context);
  JSValue json = JS_JSONStringify(context, argc > 0 ? argv[0] : JS_UNDEFINED, JS_UNDEFINED, JS_UNDEFINED);
  if (JS_IsException(json)) return json;
  /* No argument, or none JSON holds, is null. */
  size_t len = 4;
  const char *arguments = JS_IsUndefined(json) ? "null" : JS_ToCStringLen(context, &len, json);
  if (!arguments) {
    JS_FreeValue(context, json);
    return JS_EXCEPTION;
  }
  Call *call = js_mallocz(context, sizeof(Call));
  char *copy = call ? js_malloc(context, len + 1) : NULL;
  if (copy) {
    memcpy(copy, arguments, len);
    copy[len] = 0;
  }
  if (!JS_IsUndefined(json)) JS_FreeCString(context, arguments);
  JS_FreeValue(context, json);
  if (!copy) {
    js_free(context, call);
    return JS_EXCEPTION;
  }
  JSValue resolving[2];
  JSValue promise = JS_NewPromiseCapability(context, resolving);
  if (JS_IsException(promise)) {
    js_free(context, copy);
    js_free(context, call);
    return promise;
  }
  call->id = ++engine->next_id;
  call->function = (uint32_t)magic;
  call->arguments = (Text){copy, len};
  call->resolve = resolving[0];
  call->reject = resolving[1];
  if (engine->last_call) engine->last_call->next = call;
  else engine->calls = call;
  engine->last_call = call;
  return promise;
}

/* `console.log`, `info`, `warn`, `error` and `debug`: a line of the output, its values apart by spaces — a string as
   it is, an object as JSON, anything else as its text. */
static JSValue console_line(JSContext *context, JSValueConst this_value, int argc, JSValueConst *argv) {
  (void)this_value;
  Engine *engine = JS_GetContextOpaque(context);
  size_t start = engine->output.len;
  for (int i = 0; i < argc; i++) {
    if (i > 0 && append(engine->runtime, &engine->output, " ", 1)) goto fail;
    JSValue shown = JS_IsString(argv[i]) || !JS_IsObject(argv[i])
                        ? JS_DupValue(context, argv[i])
                        : JS_JSONStringify(context, argv[i], JS_UNDEFINED, JS_UNDEFINED);
    if (JS_IsException(shown)) goto fail;
    size_t len = 0;
    const char *text = JS_ToCStringLen(context, &len, shown);
    JS_FreeValue(context, shown);
    if (!text) goto fail;
    int appended = append(engine->runtime, &engine->output, text, len);
    JS_FreeCString(context, text);
    if (appended) goto fail;
  }
  if (engine->line_count == engine->line_capacity) {
    size_t capacity = engine->line_capacity ? engine->line_capacity * 2 : 16;
    size_t *grown = js_realloc(context, engine->line_ends, capacity * sizeof(size_t));
    if (!grown) goto fail;
    engine->line_ends = grown;
    engine->line_capacity = capacity;
  }
  engine->line_ends[engine->line_count++] = engine->output.len;
  return JS_UNDEFINED;
fail:
  /* The line is no part of the output; its values' exception, or the memory's, is the script's. */
  engine->output.len = start;
  return JS_HasException(context) ? JS_EXCEPTION : JS_ThrowOutOfMemory(context);
}

/* The object at `path` — `a.b` — under `root`, made where it is missing. */
static JSValue object_at(JSContext *context, JSValueConst root, const char *path, size_t len) {
  JSValue at = JS_DupValue(context, root);
  for (size_t start = 0, i = 0; i <= len; i++) {
    if (i < len && path[i] != '.') continue;
    JSAtom name = JS_NewAtomLen(context, path + start, i - start);
    JSValue next = JS_GetProperty(context, at, name);
    if (!JS_IsObject(next)) {
      JS_FreeValue(context, next);
      next = JS_NewObject(context);
      JS_SetProperty(context, at, name, JS_DupValue(context, next));
    }
    JS_FreeAtom(context, name);
    JS_FreeValue(context, at);
    at = next;
    start = i + 1;
  }
  return at;
}

/* Puts each of the host's functions — `group.name` — under the global object it names, and the console. */
static void install(Engine *engine, JSContext *context) {
  JSValue global = JS_GetGlobalObject(context);
  JSValue named = JS_NewObject(context);
  for (size_t i = 0; i < engine->function_count; i++) {
    const char *name = engine->functions[i];
    const char *dot = strrchr(name, '.');
    JSValue holder = dot ? object_at(context, named, name, (size_t)(dot - name)) : JS_DupValue(context, named);
    const char *leaf = dot ? dot + 1 : name;
    JS_SetPropertyStr(context, holder, leaf, JS_NewCFunctionMagic(context, call_function, leaf, 1, JS_CFUNC_generic_magic, (int)i));
    JS_FreeValue(context, holder);
  }
  JS_SetPropertyStr(context, global, engine->namespace_name, named);
  JSValue console = JS_NewObject(context);
  const char *levels[] = {"log", "info", "warn", "error", "debug"};
  for (size_t i = 0; i < sizeof levels / sizeof levels[0]; i++)
    JS_SetPropertyStr(context, console, levels[i], JS_NewCFunction(context, console_line, levels[i], 1));
  JS_SetPropertyStr(context, global, "console", console);
  JS_FreeValue(context, global);
}

static char *copied(EngineName name) {
  char *copy = malloc(name.len + 1);
  if (!copy) return NULL;
  memcpy(copy, name.bytes, name.len);
  copy[name.len] = 0;
  return copy;
}

void engine_free(Engine *engine);

/* An engine for a script: the global object the host's functions are under, by its name, and the functions, each
   `group.name` under it; its memory in bytes and its steps; the seed of its random numbers. NULL with no memory. */
Engine *engine_new(EngineName namespace_name, const EngineName *functions, size_t function_count, size_t memory_max,
                   uint64_t steps_max, uint64_t seed) {
  Engine *engine = calloc(1, sizeof(Engine));
  if (!engine) return NULL;
  engine->body = JS_UNDEFINED;
  engine->steps_max = steps_max;
  engine->state = ENGINE_WAITING;
  seed_random(engine, seed);
  engine->namespace_name = copied(namespace_name);
  engine->functions = calloc(function_count ? function_count : 1, sizeof(char *));
  if (!engine->namespace_name || !engine->functions) goto fail;
  for (; engine->function_count < function_count; engine->function_count++) {
    engine->functions[engine->function_count] = copied(functions[engine->function_count]);
    if (!engine->functions[engine->function_count]) goto fail;
  }
  engine->runtime = JS_NewRuntime();
  if (!engine->runtime) goto fail;
  JS_SetMemoryLimit(engine->runtime, memory_max);
  JS_SetMaxStackSize(engine->runtime, 256 * 1024);
  JS_SetInterruptHandler(engine->runtime, interrupt, engine);
  engine->context = JS_NewContext(engine->runtime);
  if (!engine->context) goto fail;
  JS_SetContextOpaque(engine->context, engine);
  install(engine, engine->context);
  if (install_clock(engine->context)) goto fail;
  return engine;
fail:
  engine_free(engine);
  return NULL;
}

/* What the script threw, kept: an Error's name, message and stack, a column on the body's first line told as the
   script's own; any other value its JSON as the message. */
static void keep_thrown(Engine *engine, JSValueConst error);

static void throw_with(Engine *engine, const char *name, const char *message) {
  text_of(&engine->thrown_name, name, strlen(name));
  text_of(&engine->thrown_message, message, strlen(message));
  text_of(&engine->thrown_stack, "", 0);
  engine->state = ENGINE_THREW;
}

/* The state the script is in once an exception came: out of its steps, or what it threw. */
static int failed(Engine *engine) {
  if (engine->out_of_steps) return engine->state = ENGINE_OUT_OF_STEPS;
  JSValue error = JS_GetException(engine->context);
  keep_thrown(engine, error);
  JS_FreeValue(engine->context, error);
  return engine->state;
}

/* What the script returned, kept as JSON — nothing is null; a value JSON cannot hold, as a BigInt, what saying so
   threw. */
static int keep_returned(Engine *engine) {
  JSContext *context = engine->context;
  JSValue value = JS_PromiseResult(context, engine->body);
  JSValue json = JS_JSONStringify(context, value, JS_UNDEFINED, JS_UNDEFINED);
  JS_FreeValue(context, value);
  if (JS_IsException(json)) return failed(engine);
  size_t len = 4;
  const char *text = JS_IsUndefined(json) ? "null" : JS_ToCStringLen(context, &len, json);
  if (!text) {
    JS_FreeValue(context, json);
    return failed(engine);
  }
  int made = text_of(&engine->returned, text, len);
  if (!JS_IsUndefined(json)) JS_FreeCString(context, text);
  JS_FreeValue(context, json);
  if (made) {
    throw_with(engine, "InternalError", "out of memory");
    return engine->state;
  }
  return engine->state = ENGINE_RETURNED;
}

/* Runs the script on, its clock telling `time`, until it waits on calls, returns, throws or spends its steps. */
int engine_step(Engine *engine, int64_t time) {
  if (engine->state != ENGINE_WAITING) return engine->state;
  JS_UpdateStackTop(engine->runtime);
  engine->now = time;
  for (;;) {
    JSContext *context;
    int ran = JS_ExecutePendingJob(engine->runtime, &context);
    if (ran == 0) break;
    if (ran < 0) return failed(engine);
  }
  if (engine->out_of_steps) return engine->state = ENGINE_OUT_OF_STEPS;
  switch (JS_PromiseState(engine->context, engine->body)) {
    case JS_PROMISE_FULFILLED:
      return keep_returned(engine);
    case JS_PROMISE_REJECTED: {
      JSValue error = JS_PromiseResult(engine->context, engine->body);
      keep_thrown(engine, error);
      JS_FreeValue(engine->context, error);
      return engine->state;
    }
    default:
      if (engine->calls) return ENGINE_WAITING;
      throw_with(engine, "Error", "the script waits on nothing that will settle");
      return engine->state;
  }
}

/*
 * What a script's body is wrapped in: an async function, so it awaits and returns, opened on the body's first line so
 * the lines stay the script's. A column the engine tells on that line is past the opening, and is told less it.
 */
static const char opening[] = "(async () => {";
static const char closing[] = "\n})";
static const char first_line[] = "script.js:1:";

/* Runs `source`, JavaScript, from its start, its clock telling `time`, as engine_step runs it on. */
int engine_run(Engine *engine, const char *source, size_t len, int64_t time) {
  JS_UpdateStackTop(engine->runtime);
  engine->now = time;
  Buffer wrapped = {0};
  if (append(engine->runtime, &wrapped, opening, sizeof opening - 1) || append(engine->runtime, &wrapped, source, len) ||
      append(engine->runtime, &wrapped, closing, sizeof closing - 1)) {
    js_free_rt(engine->runtime, wrapped.bytes);
    throw_with(engine, "InternalError", "out of memory");
    return engine->state;
  }
  JSValue body = JS_Eval(engine->context, wrapped.bytes, wrapped.len, "script.js", JS_EVAL_TYPE_GLOBAL);
  js_free_rt(engine->runtime, wrapped.bytes);
  if (JS_IsException(body)) return failed(engine);
  /* Called from here, so no frame of what wraps it is in an error's stack. */
  engine->body = JS_Call(engine->context, body, JS_UNDEFINED, 0, NULL);
  JS_FreeValue(engine->context, body);
  if (JS_IsException(engine->body)) {
    engine->body = JS_UNDEFINED;
    return failed(engine);
  }
  return engine_step(engine, time);
}

/* Hands the host the calls the script made since it was last handed them, oldest first. */
void engine_calls(Engine *engine, EngineCallFn *each, void *to) {
  for (Call *call = engine->calls; call; call = call->next) {
    if (call->reported) continue;
    const char *function = engine->functions[call->function];
    each(to, call->id, function, strlen(function), call->arguments.bytes, call->arguments.len);
    call->reported = 1;
  }
}

/* Settles call `id`: with the JSON `text` where `ok`, else rejected with an Error whose message is `text`. 0, or -1
   where the script waits on no such call. */
int engine_settle(Engine *engine, uint32_t id, int ok, const char *text, size_t len) {
  Call *previous = NULL;
  Call *call = engine->calls;
  while (call && call->id != id) previous = call, call = call->next;
  if (!call) return -1;
  if (previous) previous->next = call->next;
  else engine->calls = call->next;
  if (engine->last_call == call) engine->last_call = previous;
  JSContext *context = engine->context;
  JS_UpdateStackTop(engine->runtime);
  JSValue value;
  if (ok) {
    /* The engine's parser reads to a NUL, which the host's bytes need not end with. */
    char *terminated = js_malloc(context, len + 1);
    if (terminated) {
      memcpy(terminated, text, len);
      terminated[len] = 0;
      value = JS_ParseJSON(context, terminated, len, "settled");
      js_free(context, terminated);
    } else {
      value = JS_EXCEPTION;
    }
    if (JS_IsException(value)) {
      value = JS_GetException(context);
      ok = 0;
    }
  } else {
    value = JS_NewError(context);
    JS_SetPropertyStr(context, value, "message", JS_NewStringLen(context, text, len));
  }
  JSValue settled = JS_Call(context, ok ? call->resolve : call->reject, JS_UNDEFINED, 1, (JSValueConst *)&value);
  JS_FreeValue(context, settled);
  JS_FreeValue(context, value);
  JS_FreeValue(context, call->resolve);
  JS_FreeValue(context, call->reject);
  js_free(context, call->arguments.bytes);
  js_free(context, call);
  return 0;
}

/* `name` of `object` as a text, empty where it has none or reading it throws. */
static void property_text(JSContext *context, JSValueConst object, const char *name, Text *text) {
  JSValue value = JS_GetPropertyStr(context, object, name);
  size_t len = 0;
  const char *bytes = JS_IsUndefined(value) || JS_IsException(value) ? NULL : JS_ToCStringLen(context, &len, value);
  JS_FreeValue(context, value);
  if (!bytes) JS_FreeValue(context, JS_GetException(context));
  text_of(text, bytes ? bytes : "", bytes ? len : 0);
  if (bytes) JS_FreeCString(context, bytes);
}

/* An error's stack, a column on the body's first line told as the script's own: less the opening's, from 1. */
static void stack_text(Engine *engine, JSValueConst error) {
  Text raw = {0};
  property_text(engine->context, error, "stack", &raw);
  const char *stack = raw.bytes ? raw.bytes : "";
  size_t len = raw.len;
  const size_t mark = sizeof first_line - 1;
  const unsigned long opened = sizeof opening - 1;
  Text *out = &engine->thrown_stack;
  free(out->bytes);
  out->bytes = malloc(len + 24);
  out->len = 0;
  for (size_t i = 0; out->bytes && i < len;) {
    if (len - i > mark && memcmp(stack + i, first_line, mark) == 0 && isdigit((unsigned char)stack[i + mark])) {
      char *after = NULL;
      unsigned long column = strtoul(stack + i + mark, &after, 10);
      memcpy(out->bytes + out->len, first_line, mark);
      out->len += mark;
      /* The engine tells the first line's columns from 0 and the others' from 1: the script's first line is told from
         1, as the others are, less the opening. Never longer than it was. */
      out->len += (size_t)snprintf(out->bytes + out->len, 21, "%lu", column >= opened ? column - opened + 1 : 1);
      i = (size_t)(after - stack);
    } else {
      out->bytes[out->len++] = stack[i++];
    }
  }
  if (out->bytes) out->bytes[out->len] = 0;
  free(raw.bytes);
}

static void keep_thrown(Engine *engine, JSValueConst error) {
  JSContext *context = engine->context;
  engine->state = ENGINE_THREW;
  if (JS_IsError(error)) {
    property_text(context, error, "name", &engine->thrown_name);
    property_text(context, error, "message", &engine->thrown_message);
    stack_text(engine, error);
    return;
  }
  JSValue json = JS_JSONStringify(context, error, JS_UNDEFINED, JS_UNDEFINED);
  size_t len = 0;
  const char *text = JS_IsException(json) || JS_IsUndefined(json) ? NULL : JS_ToCStringLen(context, &len, json);
  text_of(&engine->thrown_name, "Thrown", 6);
  text_of(&engine->thrown_message, text ? text : "undefined", text ? len : 9);
  text_of(&engine->thrown_stack, "", 0);
  if (text) JS_FreeCString(context, text);
  /* A value JSON cannot hold leaves the message "undefined", its exception no one's. */
  if (JS_IsException(json)) JS_FreeValue(context, JS_GetException(context));
  JS_FreeValue(context, json);
}

/* Hands the host the JSON of what the script returned, where it returned. */
void engine_returned(Engine *engine, EngineTextFn *each, void *to) {
  if (engine->state == ENGINE_RETURNED) each(to, engine->returned.bytes, engine->returned.len);
}

/* Hands the host what the script threw — an error's name, message and stack — where it threw. */
void engine_thrown(Engine *engine, EngineThrownFn *each, void *to) {
  if (engine->state != ENGINE_THREW) return;
  Text *name = &engine->thrown_name, *message = &engine->thrown_message, *stack = &engine->thrown_stack;
  each(to, name->bytes ? name->bytes : "", name->len, message->bytes ? message->bytes : "", message->len,
       stack->bytes ? stack->bytes : "", stack->len);
}

/* Hands the host the lines the script wrote since it was last handed them. */
void engine_output(Engine *engine, EngineTextFn *each, void *to) {
  for (size_t i = 0, start = 0; i < engine->line_count; start = engine->line_ends[i++])
    each(to, engine->output.bytes + start, engine->line_ends[i] - start);
  engine->output.len = 0;
  engine->line_count = 0;
}

uint64_t engine_steps(const Engine *engine) { return engine->steps; }

/* Frees the engine and all it holds. */
void engine_free(Engine *engine) {
  if (!engine) return;
  if (engine->runtime) JS_UpdateStackTop(engine->runtime);
  while (engine->calls) {
    Call *call = engine->calls;
    engine->calls = call->next;
    JS_FreeValue(engine->context, call->resolve);
    JS_FreeValue(engine->context, call->reject);
    js_free_rt(engine->runtime, call->arguments.bytes);
    js_free_rt(engine->runtime, call);
  }
  if (engine->context) {
    JS_FreeValue(engine->context, engine->body);
    JS_FreeContext(engine->context);
  }
  if (engine->runtime) {
    js_free_rt(engine->runtime, engine->output.bytes);
    js_free_rt(engine->runtime, engine->line_ends);
    JS_FreeRuntime(engine->runtime);
  }
  for (size_t i = 0; i < engine->function_count; i++) free(engine->functions[i]);
  free(engine->functions);
  free(engine->namespace_name);
  free(engine->returned.bytes);
  free(engine->thrown_name.bytes);
  free(engine->thrown_message.bytes);
  free(engine->thrown_stack.bytes);
  free(engine);
}
