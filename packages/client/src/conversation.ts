/**
 * A thread as the interface shows it, put together by the ids that tie its parts: its turns, each the inputs it took,
 * the model's requests it made — what streamed of each while it was answered, the reply that came of it — and the calls
 * each reply made, each with the result it came to and what it asks the person while it waits; and what waits on the
 * person. The log says what happened, the status what is happening now and the streams what is being written, by the
 * same ids: a request's, a call's.
 *
 * A turn that ended never changes: it is put together once and kept, the same object from one view of the thread to
 * the next. The turn that runs is put together again only as its run of the log grows; what the status and the streams
 * say of it is laid over it, each part they change copied and every other part kept, so a reader that keeps what it
 * made of a part makes it again only where the part changed.
 */
import type { Ask, CallKey, Entry, Inbound, Item, Status, Streamed } from "./protocol.ts";
import type { ThreadView } from "./thread.ts";

type Of<Kind extends Entry["type"]> = Extract<Entry, { type: Kind }>;

/**
 * A call: the model's id of it, and Siphon's key of it — by which its result, what it asks the person and a sub-thread
 * it made name it — its tool and its arguments; its result, once it came; what it asks the person, while it waits on
 * them.
 */
export type Call = {
	readonly id: string;
	readonly key: string;
	readonly name: string;
	readonly arguments: string;
	readonly result: Of<"result"> | undefined;
	readonly ask: Ask | undefined;
};

/** A request a turn made of its model: its id; what streamed while it was answered; the reply that came of it, which says whether it failed. */
export type Step = {
	readonly request: string;
	readonly streaming: readonly Streamed[];
	readonly reply: Of<"reply"> | undefined;
	readonly calls: readonly Call[];
};

/** A turn: what its model was told as it began; its inputs; its steps; its end, once it ended. */
export type Turn = {
	readonly told: readonly Of<"system">[];
	readonly inputs: readonly Of<"input">[];
	readonly steps: readonly Step[];
	readonly end: Of<"turn-end"> | undefined;
};

/** A thread as the interface shows it. */
export type Conversation = {
	/** Its turns, from the earliest the client holds: the first may be begun before the entries held reach back. */
	readonly turns: readonly Turn[];
	/**
	 * Its compactions, in the order of its log: the thread's, none a turn's own — made as a turn's window filled, or as
	 * the person asked — each placed by its id, as every entry's is minted in order.
	 */
	readonly compactions: readonly Of<"compaction">[];
	/** Whether its latest entries were read: till they are, it holds none, though the thread may. */
	readonly read: boolean;
	/** Whether the turns reach the log's start: none is left to read further back. */
	readonly complete: boolean;
	/** What it asks the person, each beside its call where a call asks it. */
	readonly asks: readonly Ask[];
	/** The inputs that wait in its inbox for its next turn, by their ids, in the order they came. */
	readonly inbox: readonly (readonly [string, Inbound])[];
	readonly status: Status | undefined;
	/** Whether the thread is gone, as it was deleted. */
	readonly gone: boolean;
};

/** The call an ask is made for, by Siphon's key of it, as the ask's payload names it; none for an ask of no call's. */
export function callOfAsk(ask: Ask): string | undefined {
	const { payload } = ask;
	if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return undefined;
	const call = payload["call"];
	return typeof call === "string" ? call : undefined;
}

type BuildingStep = { request: string; streaming: readonly Streamed[]; reply: Of<"reply"> | undefined; calls: Call[] };
type BuildingTurn = { told: Of<"system">[]; inputs: Of<"input">[]; steps: BuildingStep[]; end: Of<"turn-end"> | undefined };

/** What streamed of a request nothing streamed of: one for all, as a part that is the same is the same object. */
const NOTHING_STREAMED: readonly Streamed[] = [];

/** Each view's conversation, made once: a view is the same object until its thread changes. */
const made = new WeakMap<ThreadView, Conversation>();

/**
 * Each run of the log put together, by its last entry, with its first and how many entries it holds: a run is the same
 * where it begins and ends with the same entries and holds as many, as the log is appended to alone — a turn that ended
 * by its end, the turn that runs before what the status and the streams say of it.
 */
const runs = new WeakMap<Entry, { readonly first: Entry; readonly length: number; readonly turn: Turn | undefined }>();

/** The conversation of a thread as `view` holds it. */
export function conversationOf(view: ThreadView): Conversation {
	const known = made.get(view);
	if (known !== undefined) return known;
	const asks = view.status?.asks ?? [];
	const turns = turnsOf(view.entries);
	const last = turns.at(-1);
	if (last !== undefined && last.end === undefined) turns[turns.length - 1] = running(last, asks, view.streaming);
	const conversation: Conversation = {
		turns,
		compactions: view.entries.filter((entry): entry is Of<"compaction"> => entry.type === "compaction"),
		read: view.read,
		complete: view.complete,
		asks,
		inbox: view.status?.inbox ?? [],
		status: view.status,
		gone: view.gone,
	};
	made.set(view, conversation);
	return conversation;
}

/** The turns the log's `entries` hold: each run through a turn's end its turn, and the run after the last end the turn that runs, where it holds one. */
function turnsOf(entries: readonly Entry[]): Turn[] {
	const turns: Turn[] = [];
	let from = 0;
	entries.forEach((entry, at) => {
		if (entry.type !== "turn-end") return;
		const turn = runOf(entries, from, at);
		if (turn !== undefined) turns.push(turn);
		from = at + 1;
	});
	const runningTurn = from < entries.length ? runOf(entries, from, entries.length - 1) : undefined;
	if (runningTurn !== undefined) turns.push(runningTurn);
	return turns;
}

/** The turn the run of `entries` from `from` through `through` holds, as it was put together where the run is the same; none where it holds none. */
function runOf(entries: readonly Entry[], from: number, through: number): Turn | undefined {
	const first = entries[from];
	const last = entries[through];
	if (first === undefined || last === undefined) return undefined;
	const length = through - from + 1;
	const known = runs.get(last);
	if (known !== undefined && known.first === first && known.length === length) return known.turn;
	const turn = turnOf(entries.slice(from, through + 1));
	runs.set(last, { first, length, turn });
	return turn;
}

/** The turn a run of the log's entries holds, from its inputs to its end — what its model was told before them its own; none where the run holds none. */
function turnOf(run: readonly Entry[]): Turn | undefined {
	const told: Of<"system">[] = [];
	let turn: BuildingTurn | undefined;
	// The calls of the turn by Siphon's keys of them: each result settles one.
	const calls = new Map<string, Call>();
	const open = (): BuildingTurn => {
		turn ??= { told, inputs: [], steps: [], end: undefined };
		return turn;
	};
	for (const entry of run) {
		switch (entry.type) {
			case "system":
				// What a turn's model is told as it begins comes before its inputs, and is told anew within it after a recap:
				// the turn's either way, as it takes what was told before it opened.
				told.push(entry);
				break;
			case "input":
				open().inputs.push(entry);
				break;
			case "reply": {
				const step = stepOf(open(), entry.request);
				step.reply = entry;
				step.calls = callsOf(entry.response.output, entry.calls ?? []);
				for (const call of step.calls) calls.set(call.key, call);
				break;
			}
			case "result": {
				const building = open();
				const call = calls.get(entry.call);
				if (call === undefined) break;
				const settled = { ...call, result: entry };
				calls.set(call.key, settled);
				for (const step of building.steps) step.calls = step.calls.map((each) => (each === call ? settled : each));
				break;
			}
			case "turn-end":
				open().end = entry;
				break;
			default:
			// How the thread runs is its config's to show, and its compactions the conversation's; a discarded reply is none.
		}
	}
	return turn;
}

/** The step of the request `request` in `turn`, made where the turn has none yet. */
function stepOf(turn: BuildingTurn, request: string): BuildingStep {
	const found = turn.steps.find((step) => step.request === request);
	if (found !== undefined) return found;
	const step: BuildingStep = { request, streaming: NOTHING_STREAMED, reply: undefined, calls: [] };
	turn.steps.push(step);
	return step;
}

/** The calls a reply made, as it keyed them, in the order its model made them: each with its function and its arguments as its output gives them. */
function callsOf(output: readonly Item[], keys: readonly CallKey[]): Call[] {
	const made = new Map(output.flatMap((item) => (item.type === "function_call" ? [[item.call_id, item] as const] : [])));
	return keys.flatMap(({ call_id: id, key }) => {
		const item = made.get(id);
		return item === undefined ? [] : [{ id, key, name: item.name, arguments: item.arguments, result: undefined, ask: undefined }];
	});
}

/**
 * The turn that runs, as the status and the streams say of it: the requests it waits on its model for as steps of their
 * own, what streamed of each, and what each call asks the person — each part they change copied, every other kept.
 */
function running(turn: Turn, asks: readonly Ask[], streaming: ReadonlyMap<string, readonly Streamed[]>): Turn {
	const asked = new Map(
		asks.flatMap((ask) => {
			const call = callOfAsk(ask);
			return call === undefined ? [] : [[call, ask] as const];
		}),
	);
	const steps = keptWhereSame(turn.steps, (step) => {
		const calls = keptWhereSame(step.calls, (call) => {
			const ask = call.result === undefined ? asked.get(call.key) : undefined;
			return ask === call.ask ? call : { ...call, ask };
		});
		return calls === step.calls ? step : { ...step, calls };
	});
	const added = [...streaming]
		.filter(([request]) => !turn.steps.some((step) => step.request === request))
		.map(([request, streamed]) => ({ request, streaming: streamed, reply: undefined, calls: [] }));
	if (steps === turn.steps && added.length === 0) return turn;
	return { ...turn, steps: [...steps, ...added] };
}

/** `parts`, each as `next` makes it: the same array where it made each the same. */
function keptWhereSame<Part>(parts: readonly Part[], next: (part: Part) => Part): readonly Part[] {
	let changed = false;
	const remade = parts.map((part) => {
		const each = next(part);
		if (each !== part) changed = true;
		return each;
	});
	return changed ? remade : parts;
}
