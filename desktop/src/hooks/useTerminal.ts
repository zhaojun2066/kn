/**
 * useTerminal — Multi-instance terminal hook.
 *
 * This file is a thin re-export layer. The implementation lives in ./useTerminal/.
 */
export { useTerminal } from "./useTerminal/index";
export type { SessionRecord } from "./useTerminal/types";
export { parseAiCmd } from "./useTerminal/utils";
