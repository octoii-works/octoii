/** The actual completed calls of an execute script, kept with its result rather than inferred from its code. */
import type { JsonValue } from "@octoii-works/octoii-client";
import { type Static, Type } from "typebox";
import { matches } from "./schema.ts";

// These values are already JSON from the protocol; only the execution envelope needs further validation here.
const Json = Type.Unsafe<JsonValue>({});
const ExecutedCall = Type.Object({
	id: Type.Integer({ minimum: 0, maximum: 4_294_967_295 }),
	key: Type.String(),
	tool: Type.String(),
	arguments: Type.String(),
	status: Type.Union([
		Type.Literal("ok"),
		Type.Literal("invalid-arguments"),
		Type.Literal("unavailable"),
		Type.Literal("tool-error"),
		Type.Literal("interrupted"),
		Type.Literal("rejected"),
	]),
	output: Type.String(),
	structuredContent: Type.Optional(Json),
	metadata: Type.Optional(Json),
});
const Execution = Type.Object({ calls: Type.Array(ExecutedCall) });

/** A completed nested call. It has no invented timing, images or second copy of delivered resources. */
export type ExecutedCall = Static<typeof ExecutedCall>;

/** Only a completed execute result carries nested calls; an unfinished or malformed result carries none. */
export function executedCallsOf(tool: string, result: JsonValue | null | undefined): readonly ExecutedCall[] {
	return tool === "execute" && matches(Execution, result) ? result.calls : [];
}
