/**
 * The timeline's rows: each turn's words, its work's fold, its reply and its end — with the
 * room each leaves above it, what is the thread's own between turns a row each; and rows that did not change kept as
 * the same objects.
 */
import { describe, expect, it } from "bun:test";
import type { Listed } from "@octoii-works/octoii-client";
import { rowsOf, steadyRows, type TimelineRow } from "../src/core/rows.ts";
import type { CallLine, Said, TimelineItem, TimelineTurn } from "../src/core/timeline.ts";

const said = (key: string, text: string): Said => ({ key, delivery: `d-${key}`, at: 0, from: undefined, text, attachments: [] });

const callOf = (id: string): CallLine => ({
	kind: "call",
	id,
	key: `k${id}`,
	step: "s1",
	thread: undefined,
	tool: "read",
	work: "read",
	verb: "Read",
	subject: "a.ts",
	code: false,
	aside: undefined,
	trouble: undefined,
	arguments: "{}",
	output: "",
	files: [],
	structuredContent: undefined,
	began: 1_000,
	ended: 2_000,
	state: "done",
	ask: undefined,
	calls: [],
});

/** A turn as the page shows it: `given` over one that the person began with "Fix it" and that replied. */
const turnOf = (given: Partial<TimelineTurn> = {}): TimelineTurn => ({
	key: "t1",
	said: [said("i1", "Fix it")],
	work: [],
	answer: undefined,
	changes: [],
	writing: { id: undefined, words: "", calls: [] },
	asking: undefined,
	asks: [],
	ending: { state: "replied" },
	startedAt: 1_000,
	endedAt: 9_000,
	...given,
});

const shape = (rows: readonly TimelineRow[]): readonly string[] => rows.map((row) => `${row.kind} ${row.room} ${row.key}`);

describe("the timeline's rows", () => {
	it("groups a turn's work under its duration while keeping its reply and end in separate rows", () => {
		const turn = turnOf({
			said: [said("i1", "Fix it"), said("i2", "And test it")],
			work: [
				{ kind: "thought", key: "r1:thought", text: "Look first.", took: 900 },
				callOf("c1"),
				{ kind: "words", key: "r2", text: "Reading on." },
				callOf("c2"),
			],
			answer: { key: "r3", text: "Fixed." },
		});
		const rows = rowsOf([{ kind: "turn", key: turn.key, turn }]);
		expect(shape(rows)).toEqual(["said turn said:i1", "said message said:i2", "work message work:t1", "reply part words:r3", "end part end:t1"]);
		const work = rows.find((row) => row.kind === "work");
		expect(work?.work.label).toBe("Worked for 8s");
		expect(work?.work.working).toBe(false);
		expect(work?.work.parts.map((part) => part.kind)).toEqual(["calls", "step", "calls"]);
		const end = rows.at(-1);
		expect(end?.kind === "end" ? end.end.foot?.worked : undefined).toBeUndefined();
	});

	it("ends a turn that works with what it does now, one that ended with its changes and foot, and a part the person steered on with neither", () => {
		const working = turnOf({ ending: { state: "working" }, endedAt: undefined, asking: 2_000, changes: [{ path: "a.ts", added: 1, removed: 0 }] });
		const [, workingEnd] = rowsOf([{ kind: "turn", key: "t1", turn: working }]);
		expect(workingEnd?.kind === "end" ? [workingEnd.end.doing?.words, workingEnd.end.foot, workingEnd.end.changes.length] : []).toEqual([
			"Thinking",
			undefined,
			0,
		]);
		const ended = turnOf({ answer: { key: "r1", text: "Done." }, changes: [{ path: "a.ts", added: 1, removed: 0 }] });
		const endedEnd = rowsOf([{ kind: "turn", key: "t1", turn: ended }]).at(-1);
		expect(endedEnd?.kind === "end" ? [endedEnd.end.doing, endedEnd.end.foot?.reply, endedEnd.end.foot?.worked, endedEnd.end.changes.length] : []).toEqual([
			undefined,
			"Done.",
			"Worked for 8s",
			1,
		]);
		const steered = turnOf({ ending: { state: "steered" } });
		expect(shape(rowsOf([{ kind: "turn", key: "t1", turn: steered }]))).toEqual(["said turn said:i1"]);
	});

	it("gives what is the thread's own between turns a row each, with a turn's room above it", () => {
		const items: TimelineItem[] = [
			{ kind: "turn", key: "t1", turn: turnOf() },
			{ kind: "summed-up", key: "k1" },
			{ kind: "limited", key: "limited", limited: { reason: "used-up" } as never, resumingAt: undefined },
		];
		expect(shape(rowsOf(items)).slice(-2)).toEqual(["summed-up turn summed-up:k1", "limited turn limited"]);
	});

	it("keeps each row that did not change the same object, and gives a changed one anew", () => {
		// The client keeps an input and a call the same objects, and the page reads each once: the words and the line are the same.
		const words = said("i1", "Fix it");
		const line = callOf("c1");
		const working = (given: Partial<TimelineTurn>): TimelineItem => ({
			kind: "turn",
			key: "t1",
			turn: turnOf({ said: [words], ending: { state: "working" }, endedAt: undefined, ...given }),
		});
		const held = steadyRows(rowsOf([working({ work: [line] })]), new Map());
		// Words being written are a reply row of their own, known by the request writing them; the rows that were stay the same objects.
		const writing = steadyRows(rowsOf([working({ work: [line], writing: { id: "q1", words: "Fix", calls: [] } })]), held.byKey).rows;
		expect(writing.map((row) => row.key)).toEqual(["said:i1", "work:t1", "words:q1", "end:t1"]);
		expect(writing[2]?.kind === "reply" ? [writing[2].text, writing[2].writing] : []).toEqual(["Fix", true]);
		expect([writing[0], writing[1], writing[3]]).toEqual([held.rows[0], held.rows[1], held.rows[2]] as TimelineRow[]);
		[writing[0], writing[1], writing[3]].forEach((row, place) => expect(row).toBe(held.rows[place] as TimelineRow));
		// New work grows the existing group; its stable key keeps a reader's manual fold choice.
		const grown = steadyRows(rowsOf([working({ work: [line, { kind: "web-search", key: "w1", query: "zig" }] })]), held.byKey).rows;
		expect(grown.map((row) => row.key)).toEqual(["said:i1", "work:t1", "end:t1"]);
		expect(grown[0]).toBe(held.rows[0] as TimelineRow);
		expect(grown[1]).not.toBe(held.rows[1] as TimelineRow);
		expect(grown[2]).toBe(held.rows[2] as TimelineRow);
		const completed = rowsOf([{ kind: "turn", key: "t1", turn: turnOf({ said: [words], work: [line] }) }]);
		expect(completed[1]?.key).toBe(held.rows[1]?.key);
		expect(completed[1]?.kind === "work" ? completed[1].work.working : undefined).toBe(false);
	});

	it("updates a waiting sub-thread when a new request uses the same key", () => {
		const waitingRows = (id: string): readonly TimelineRow[] => {
			const thread: Listed = {
				id: "sub-thread",
				parent: { kind: "general_purpose", thread: "parent", call: "delegate" },
				config: { title: "Check the files" },
				marks: { pinned: false, archived: false },
				status: {
					turn: "running",
					began_at: 1_000,
					inbox: [],
					asks: [{ id, key: "permission", name: "siphon.permission", payload: null, answer_schema: null }],
					compacting: false,
				},
			};
			return rowsOf([{ kind: "now", key: "now", now: { waiting: [thread], terminals: [], threads: [] } }]);
		};
		const held = steadyRows(waitingRows("request-1"), new Map());
		expect(steadyRows(waitingRows("request-1"), held.byKey).rows[0]).toBe(held.rows[0]);
		const [updated] = steadyRows(waitingRows("request-2"), held.byKey).rows;
		expect(updated?.kind === "now" ? updated.now.waiting[0]?.status.asks[0]?.id : undefined).toBe("request-2");
	});

	it("uses a useful label without timestamps and keeps failure details outside completed work", () => {
		const turn = turnOf({ work: [callOf("c1")], startedAt: undefined, endedAt: undefined, ending: { state: "failed", why: "The provider refused" } });
		const rows = rowsOf([{ kind: "turn", key: "t1", turn }]);
		expect(rows[1]?.kind === "work" ? rows[1].work.label : undefined).toBe("Worked");
		expect(rows.at(-1)?.kind === "end" ? rows.at(-1) : undefined).toMatchObject({ end: { failure: "The provider refused" } });
	});
});
