import { describe, expect, it } from "bun:test";
import { ClientError } from "@octoii-works/octoii-client";
import { answerError, Answers } from "../src/core/answers.ts";

describe("answers shared by a client's request entry points", () => {
	it("admits only the first decision, even before React renders its disabled state", async () => {
		const response = Promise.withResolvers<void>();
		const sent: unknown[] = [];
		const answers = new Answers({
			answer: (_thread, _key, value) => {
				sent.push(value);
				return response.promise;
			},
		});
		const observed: boolean[] = [];
		const unsubscribe = answers.subscribe(() => observed.push(answers.pending("thread", "ask")));
		const first = answers.submit("thread", "ask", { decision: "deny", reason: "keep the file" });
		expect(answers.pending("thread", "ask")).toBe(true);
		expect(await answers.submit("thread", "ask", { decision: "allow" })).toBe(false);
		expect(sent).toEqual([{ decision: "deny", reason: "keep the file" }]);
		response.resolve();
		expect(await first).toBe(true);
		expect(observed).toEqual([true, false]);
		unsubscribe();
	});

	it("releases a failed submission for an explicit retry, without retrying on its own", async () => {
		const response = Promise.withResolvers<void>();
		let calls = 0;
		const answers = new Answers({
			answer: () => {
				calls++;
				return calls === 1 ? response.promise : Promise.resolve();
			},
		});
		const first = answers.submit("thread", "ask", { decision: "deny", reason: "keep this reason" });
		const failure = new Error("connection lost before acknowledgement");
		response.reject(failure);
		await expect(first).rejects.toThrow("connection lost before acknowledgement");
		expect(answers.pending("thread", "ask")).toBe(false);
		expect(calls).toBe(1);
		expect(await answers.submit("thread", "ask", { decision: "deny", reason: "keep this reason" })).toBe(true);
		expect(calls).toBe(2);
	});

	it("keeps different threads, requests and clients independent", async () => {
		const response = Promise.withResolvers<void>();
		const answers = new Answers({ answer: () => response.promise });
		const other = new Answers({ answer: () => Promise.resolve() });
		const pending = [answers.submit("a", "key", {}), answers.submit("b", "key", {}), answers.submit("a", "next", {})];
		expect(answers.pending("a", "key")).toBe(true);
		expect(answers.pending("b", "key")).toBe(true);
		expect(answers.pending("a", "next")).toBe(true);
		expect(await other.submit("a", "key", {})).toBe(true);
		response.resolve();
		expect(await Promise.all(pending)).toEqual([true, true, true]);
	});

	it("keeps the lock while one view unmounts and another subscribes", async () => {
		const response = Promise.withResolvers<void>();
		const answers = new Answers({ answer: () => response.promise });
		const unmount = answers.subscribe(() => undefined);
		const first = answers.submit("thread", "ask", {});
		unmount();
		expect(await answers.submit("thread", "ask", {})).toBe(false);
		const seen: boolean[] = [];
		const unsubscribe = answers.subscribe(() => seen.push(answers.pending("thread", "ask")));
		response.resolve();
		await first;
		expect(seen).toEqual([false]);
		unsubscribe();
	});
});

describe("answer failure wording", () => {
	it("does not call a lost acknowledgement a rejected answer", () => {
		expect(answerError(new ClientError("connection-lost", "Disconnected"))).toContain("Your answer may have been received");
	});

	it("preserves offline and server rejection reasons", () => {
		for (const code of ["not-connected", "not_given"]) {
			expect(answerError(new ClientError(code, "Original reason"))).toBe("Original reason");
		}
		expect(answerError("Unexpected failure")).toBe("Unexpected failure");
	});
});
