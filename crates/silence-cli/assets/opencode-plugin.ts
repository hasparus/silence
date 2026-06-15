import { execFile } from "node:child_process";
import type { Plugin } from "@opencode-ai/plugin";

const BIN = __BIN__;

function runSilence(args: string[], stdin?: string): Promise<string> {
  return new Promise((resolve) => {
    const child = execFile(BIN, args, (_err, stdout) => resolve(stdout ?? ""));
    if (stdin) child.stdin?.end(stdin);
  });
}

function noteFrom(stdout: string): string | undefined {
  const line = stdout.trim();
  if (!line) return undefined;
  try {
    const note = JSON.parse(line)?.hookSpecificOutput?.additionalContext;
    return typeof note === "string" ? note : undefined;
  } catch {
    return undefined;
  }
}

function appendNote(output: { output: string }, note: string | undefined): void {
  if (note) output.output += `\n\n${note}`;
}

function isFileTool(tool: string): tool is "write" | "edit" {
  return tool === "write" || tool === "edit";
}

function hasFilePath(args: unknown): args is { filePath: string } {
  return (
    typeof args === "object" &&
    args !== null &&
    "filePath" in args &&
    typeof (args as { filePath: unknown }).filePath === "string"
  );
}

function hasPatchText(args: unknown): args is { patchText: string } {
  return (
    typeof args === "object" &&
    args !== null &&
    "patchText" in args &&
    typeof (args as { patchText: unknown }).patchText === "string"
  );
}

export const SilencePlugin: Plugin = async () => ({
  "tool.execute.after": async (input, output) => {
    const { tool, args } = input;
    if (isFileTool(tool) && hasFilePath(args)) {
      appendNote(output, noteFrom(await runSilence(["hook", args.filePath])));
      return;
    }
    if (tool === "apply_patch" && hasPatchText(args)) {
      const stdout = await runSilence(["hook"], JSON.stringify({ patchText: args.patchText }));
      appendNote(output, noteFrom(stdout));
    }
  },
});
