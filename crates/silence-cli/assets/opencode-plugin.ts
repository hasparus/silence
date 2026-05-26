import { execFile } from "node:child_process";
import type { Plugin } from "@opencode-ai/plugin";

const BIN = __BIN__;

function runSilence(args: string[], stdin?: string): Promise<void> {
  return new Promise((resolve) => {
    const child = execFile(BIN, args, () => resolve());
    if (stdin) child.stdin?.end(stdin);
  });
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
  "tool.execute.after": async (input) => {
    const { tool, args } = input;
    if (isFileTool(tool) && hasFilePath(args)) {
      await runSilence(["hook", args.filePath]);
      return;
    }
    if (tool === "apply_patch" && hasPatchText(args)) {
      await runSilence(["hook"], JSON.stringify({ patchText: args.patchText }));
    }
  },
});
