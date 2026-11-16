/** Keep the service's executable outside the movable app bundle, without replacing a running version. */
import { access, cp, mkdir, mkdtemp, readFile, rename, rm } from "node:fs/promises";
import { join } from "node:path";

export async function installBundledRuntime(source: string, directory: string): Promise<{ program: string; version: string }> {
	const manifest: unknown = JSON.parse(await readFile(join(source, "manifest.json"), "utf8"));
	if (
		typeof manifest !== "object" ||
		manifest === null ||
		!("id" in manifest) ||
		typeof manifest.id !== "string" ||
		!/^[a-f0-9]{64}$/.test(manifest.id) ||
		!("version" in manifest) ||
		typeof manifest.version !== "string" ||
		!manifest.version
	)
		throw new Error("The bundled node manifest is invalid.");
	const target = join(directory, manifest.id);
	const program = join(target, "bin/octoii");
	try {
		await access(program);
		return { program, version: manifest.version };
	} catch (error) {
		if (!(error instanceof Error) || !("code" in error) || error.code !== "ENOENT") throw error;
	}
	await mkdir(directory, { recursive: true, mode: 0o700 });
	const temporary = await mkdtemp(join(directory, ".install-"));
	try {
		await cp(source, temporary, { recursive: true });
		await access(join(temporary, "bin/octoii"));
		await access(join(temporary, "share/octoii/app/index.html"));
		try {
			await rename(temporary, target);
		} catch (error) {
			if (!(error instanceof Error) || !("code" in error) || (error.code !== "EEXIST" && error.code !== "ENOTEMPTY")) throw error;
			// Another app instance may have completed the same immutable installation.
			await access(program);
		}
	} finally {
		await rm(temporary, { recursive: true, force: true });
	}
	return { program, version: manifest.version };
}
