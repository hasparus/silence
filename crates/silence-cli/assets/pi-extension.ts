import { execFile } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { isEditToolResult, isWriteToolResult } from "@earendil-works/pi-coding-agent";

const BIN = __BIN__;

export default function (pi: ExtensionAPI) {
  pi.on("tool_result", (event) => {
    if (event.isError) return;
    if (!isEditToolResult(event) && !isWriteToolResult(event)) return;
    execFile(BIN, ["hook", event.input.path], () => {});
  });
}
