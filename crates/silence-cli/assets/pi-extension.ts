import { execFile } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { isEditToolResult, isWriteToolResult } from "@earendil-works/pi-coding-agent";

const BIN = __BIN__;

function runSilence(args: string[]): Promise<string> {
  return new Promise((resolve) => {
    execFile(BIN, args, (_err, stdout) => resolve(stdout ?? ""));
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

export default function (pi: ExtensionAPI) {
  pi.on("tool_result", async (event) => {
    if (event.isError) return;
    if (!isEditToolResult(event) && !isWriteToolResult(event)) return;
    const note = noteFrom(await runSilence(["hook", event.input.path]));
    if (!note) return;
    return { content: [...event.content, { type: "text", text: note }] };
  });
}
