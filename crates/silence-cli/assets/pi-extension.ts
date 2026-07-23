import { execFile } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

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
    if (event.toolName !== "edit" && event.toolName !== "write") return;
    const path = event.input.path;
    if (typeof path !== "string") return;
    const note = noteFrom(await runSilence(["hook", path]));
    if (!note) return;
    return { content: [...event.content, { type: "text", text: note }] };
  });
}
