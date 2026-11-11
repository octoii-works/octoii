/* tslint:disable */
/* eslint-disable */
/**
 * The `ReadableStreamType` enum.
 *
 * *This API requires the following crate features to be activated: `ReadableStreamType`*
 */

export type ReadableStreamType = "bytes";

/**
 * A machine's answer: its status, its headers as name and value in turn, and its body.
 */
export class Answer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get body(): Uint8Array;
    set body(value: Uint8Array | number[]);
    headers: string[];
    status: number;
}

export class IntoUnderlyingByteSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableByteStreamController): Promise<any>;
    start(controller: ReadableByteStreamController): void;
    readonly autoAllocateChunkSize: number;
    readonly type: ReadableStreamType;
}

export class IntoUnderlyingSink {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    abort(reason: any): Promise<any>;
    close(): Promise<any>;
    write(chunk: any): Promise<any>;
}

export class IntoUnderlyingSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableStreamDefaultController): Promise<any>;
}

/**
 * The path a session sends on: `direct`, `relay` or `ice`, and its round trip in milliseconds.
 */
export class Path {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    rtt_ms: number;
    via: string;
}

/**
 * A session to a machine.
 */
export class Session {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Ends the session.
     */
    close(): Promise<void>;
    /**
     * The machine's key.
     */
    machine(): string;
    /**
     * Opens a session to the machine of the key `machine`, through its relay at `relay`, as a key made for this
     * session alone.
     *
     * # Errors
     * Where the key or the relay is not one, or the machine cannot be reached.
     */
    static open(relay: string, machine: string): Promise<Session>;
    /**
     * The path the session sends on now; none before it has one.
     */
    path(): Path | undefined;
    /**
     * Sends a request — its method, its path at the machine, its headers as name and value in turn, its body — and
     * gives the machine's answer.
     *
     * # Errors
     * Where the request is no request, or it could not be sent or answered.
     */
    request(method: string, path: string, headers: string[], body: Uint8Array | number[]): Promise<Answer>;
    /**
     * Opens a WebSocket at `path` at the machine, with `headers`, names and values in turn.
     *
     * # Errors
     * Where the machine refused it, or it could not be opened.
     */
    socket(path: string, headers: string[]): Promise<Socket>;
}

/**
 * A WebSocket to a machine: messages sent and received, as a page's WebSocket has them.
 */
export class Socket {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Closes the socket.
     */
    close(): Promise<void>;
    /**
     * The next message: a string for text, a `Uint8Array` for bytes; null once the socket closed.
     *
     * # Errors
     * Where the socket failed.
     */
    next(): Promise<any>;
    /**
     * Sends a message of bytes.
     *
     * # Errors
     * Where the socket is closed.
     */
    send_bytes(bytes: Uint8Array | number[]): Promise<void>;
    /**
     * Sends a message of text.
     *
     * # Errors
     * Where the socket is closed.
     */
    send_text(text: string): Promise<void>;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_answer_free: (a: number, b: number) => void;
    readonly __wbg_get_answer_body: (a: number) => [number, number];
    readonly __wbg_get_answer_headers: (a: number) => [number, number];
    readonly __wbg_get_answer_status: (a: number) => number;
    readonly __wbg_get_path_rtt_ms: (a: number) => number;
    readonly __wbg_get_path_via: (a: number) => [number, number];
    readonly __wbg_intounderlyingbytesource_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsink_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsource_free: (a: number, b: number) => void;
    readonly __wbg_path_free: (a: number, b: number) => void;
    readonly __wbg_session_free: (a: number, b: number) => void;
    readonly __wbg_set_answer_body: (a: number, b: number, c: number) => void;
    readonly __wbg_set_answer_headers: (a: number, b: number, c: number) => void;
    readonly __wbg_set_answer_status: (a: number, b: number) => void;
    readonly __wbg_set_path_rtt_ms: (a: number, b: number) => void;
    readonly __wbg_set_path_via: (a: number, b: number, c: number) => void;
    readonly __wbg_socket_free: (a: number, b: number) => void;
    readonly intounderlyingbytesource_autoAllocateChunkSize: (a: number) => number;
    readonly intounderlyingbytesource_cancel: (a: number) => void;
    readonly intounderlyingbytesource_pull: (a: number, b: any) => any;
    readonly intounderlyingbytesource_start: (a: number, b: any) => void;
    readonly intounderlyingbytesource_type: (a: number) => number;
    readonly intounderlyingsink_abort: (a: number, b: any) => any;
    readonly intounderlyingsink_close: (a: number) => any;
    readonly intounderlyingsink_write: (a: number, b: any) => any;
    readonly intounderlyingsource_cancel: (a: number) => void;
    readonly intounderlyingsource_pull: (a: number, b: any) => any;
    readonly session_close: (a: number) => any;
    readonly session_machine: (a: number) => [number, number];
    readonly session_open: (a: number, b: number, c: number, d: number) => any;
    readonly session_path: (a: number) => number;
    readonly session_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => any;
    readonly session_socket: (a: number, b: number, c: number, d: number, e: number) => any;
    readonly socket_close: (a: number) => any;
    readonly socket_next: (a: number) => any;
    readonly socket_send_bytes: (a: number, b: number, c: number) => any;
    readonly socket_send_text: (a: number, b: number, c: number) => any;
    readonly ring_core_0_17_14__bn_mul_mont: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke___js_sys_102d112e3c898ff1___Function_fn_wasm_bindgen_8795450f7808015b___JsValue_____wasm_bindgen_8795450f7808015b___sys__Undefined___js_sys_102d112e3c898ff1___Function_fn_wasm_bindgen_8795450f7808015b___JsValue_____wasm_bindgen_8795450f7808015b___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke___wasm_bindgen_8795450f7808015b___JsValue__core_37ee0ce0c4cd57d5___result__Result_____wasm_bindgen_8795450f7808015b___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke___wasm_bindgen_8795450f7808015b___JsValue______true_: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke___web_sys_f71e8b5e82ee5384___features__gen_CloseEvent__CloseEvent______true_: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke___web_sys_f71e8b5e82ee5384___features__gen_MessageEvent__MessageEvent______true_: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke_______true_: (a: number, b: number) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke_______true__1_: (a: number, b: number) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke_______true__2_: (a: number, b: number) => void;
    readonly wasm_bindgen_8795450f7808015b___convert__closures_____invoke_______true__3_: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
