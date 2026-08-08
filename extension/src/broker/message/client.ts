import type { SerDes } from "squash-ts";
import { upstreamState, type SessionId, type UpstreamState } from "./upstream";
import { domPatch, type DomPatch } from "./dom";
import { logEntries, type LogEntry } from "./log";
import { enumFamilies, opResult, type EnumFamily, type OpResult } from "./operation";
import { remoteCalls, spyConfig, type RemoteCall, type SpyConfig } from "./remote";
import { str, taggedUnion, u32, u8 } from "./serde";
import { fromBytes, textDecode, textEncode, toBytes } from "./transport";

/** Mirrors `ClientMessage::OperationResult`'s `{ id, result }` payload. */
export interface OperationResponse {
    id: number;
    result: OpResult;
}

/** A session's property write-security ordinal, as set by the `security` command. */
export interface SecurityChange {
    id: SessionId;
    level: number;
}

/** Mirrors `ClientMessage::RunCode`'s `{ session, request, source }` payload; `request` is ours to choose and comes back on the matching `ServerMessage::RunResult`. */
export interface RunRequest {
    session: SessionId;
    request: number;
    source: string;
}

/** Every inbound event the broker accepts from a client. */
export type ClientMessage =
    /** Ask the broker to shut itself down gracefully. */
    | { type: "Shutdown" }
    /** Enable or disable the broker's connection to an upstream tool. */
    | { type: "SetUpstream"; content: UpstreamState }
    /** Apply an incremental DOM patch to the session's mirror. */
    | { type: "UpdateDom"; content: DomPatch }
    /** The client's `Enum:GetEnums()` catalog, sent once on connect so the broker can resolve `DomValue::Enum` family indices back to Roblox family names. */
    | { type: "EnumFamilies"; content: EnumFamily[] }
    /** The result of applying a relayed `ServerMessage::Operation`. */
    | { type: "OperationResult"; content: OperationResponse }
    /** Ask the broker to make *this* client the active (forwarded) session. */
    | { type: "RequestActive" }
    /** A batch of the client's console output, relayed on to the control connections. */
    | { type: "Log"; content: LogEntry[] }
    /** A batch of remote calls the client's spy captured; the client keeps no copy. */
    | { type: "RemoteCalls"; content: RemoteCall[] }

    /** Control messages */

    /** Control-only: make the session with the given id active. */
    | { type: "SwapActive"; content: SessionId }
    /** Control-only: ask for the connected-session list, answered with a `ServerMessage::Sessions`. */
    | { type: "ListSessions" }
    /** Control-only: set a session's property write-security ordinal. */
    | { type: "SetSecurity"; content: SecurityChange }
    /** Control-only: ask for the buffered console history, answered with one `SessionLog` per run of lines. */
    | { type: "RequestLogs" }
    /** Control-only: compile and run `source` on the given session, answered with a `ServerMessage::RunResult`. */
    | { type: "RunCode"; content: RunRequest }
    /** Control-only: ask for the buffered remote-call history, answered with one `SessionRemotes` per run of calls. */
    | { type: "RequestRemotes" }
    /** Control-only: drop the buffered remote-call history, echoed to every control connection as `RemotesCleared`. */
    | { type: "ClearRemotes" }
    /** Control-only: change what the sessions capture, pushed on to each of them as a `ServerMessage::Spy`. */
    | { type: "SetSpy"; content: SpyConfig };

/** A struct variant, so the fields land on the wire reversed. */
const operationResponse: SerDes<OperationResponse> = {
    ser(cursor, value) {
        opResult.ser(cursor, value.result);
        u32.ser(cursor, value.id);
    },

    des(cursor) {
        const id = u32.des(cursor);
        return { id, result: opResult.des(cursor) };
    },
};

/** A struct variant, so the fields land on the wire reversed. `SessionId` is a newtype over `u32`. */
const securityChange: SerDes<SecurityChange> = {
    ser(cursor, value) {
        u8.ser(cursor, value.level);
        u32.ser(cursor, value.id);
    },

    des(cursor) {
        const id = u32.des(cursor);
        return { id, level: u8.des(cursor) };
    },
};

/** A struct variant, so the fields land on the wire reversed. */
const runRequest: SerDes<RunRequest> = {
    ser(cursor, value) {
        str.ser(cursor, value.source);
        u32.ser(cursor, value.request);
        u32.ser(cursor, value.session);
    },

    des(cursor) {
        const session = u32.des(cursor);
        const request = u32.des(cursor);
        return { session, request, source: str.des(cursor) };
    },
};

/** Mirrors `ClientMessage`. */
export const clientMessage: SerDes<ClientMessage> = taggedUnion<ClientMessage>([
    { type: "Shutdown" },
    { type: "SetUpstream", content: upstreamState },
    { type: "UpdateDom", content: domPatch },
    { type: "EnumFamilies", content: enumFamilies },
    { type: "OperationResult", content: operationResponse },
    { type: "RequestActive" },
    { type: "Log", content: logEntries },
    { type: "RemoteCalls", content: remoteCalls },
    { type: "SwapActive", content: u32 },
    { type: "ListSessions" },
    { type: "SetSecurity", content: securityChange },
    { type: "RequestLogs" },
    { type: "RunCode", content: runRequest },
    { type: "RequestRemotes" },
    { type: "ClearRemotes" },
    { type: "SetSpy", content: spyConfig },
]);

/** Encode a [`ClientMessage`] into a base64 text frame for the broker. */
export function encodeClient(message: ClientMessage): string {
    return textEncode(toBytes(clientMessage, message));
}

/** Decode a base64 text frame back into a [`ClientMessage`] — the broker's own direction, mirrored here so a frame can be round-tripped. */
export function decodeClient(frame: string): ClientMessage {
    return fromBytes(clientMessage, textDecode(frame));
}
