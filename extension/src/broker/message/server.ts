import type { SerDes } from "squash-ts";
import { upstreamState, type SessionId, type UpstreamState } from "./upstream";
import { logEntries, type LogEntry } from "./log";
import { operation, opResult, type Operation, type OpResult } from "./operation";
import { boolean, optStr, serdeArray, str, strArray, taggedUnion, u32, u8 } from "./serde";
import { fromBytes, textDecode, textEncode, toBytes } from "./transport";
import type { DomId } from "./variant";

/** Mirrors `ServerMessage::Operation`'s `{ id, op }` payload; `id` correlates the `ClientMessage::OperationResult` the client replies with. */
export interface OperationRequest {
    id: number;
    op: Operation;
}

/** Mirrors `Search`'s `{ from, query }` payload. */
export interface SearchQuery {
    from: DomId;
    query: string;
}

/** Mirrors `SessionLog`'s `{ id, entries }` payload: console output relayed from one session. */
export interface SessionLogBatch {
    id: SessionId;
    entries: LogEntry[];
}

/** Mirrors `RunResult`'s `{ session, request, result }` payload: the outcome of a `ClientMessage::RunCode`, with the `request` id we chose echoed back. */
export interface RunOutcome {
    session: SessionId;
    request: number;
    result: OpResult;
}

/** One connected client session as reported to the `sessions` command. */
export interface SessionInfo {
    id: SessionId;
    username: string | undefined;
    peer: string;
    active: boolean;
    securityLevel: number;
}

/** Every outbound event the broker sends to a client. */
export type ServerMessage =
    /** Greet a client whose connection was just accepted. */
    | { type: "Hello" }
    /** Confirm that an upstream connection was enabled or disabled. */
    | { type: "UpstreamChanged"; content: UpstreamState }
    /** Mirror a node's immediate children (lazy population); `undefined` requests the watch root's top level, bootstrapping the tree. */
    | { type: "RequestChildren"; content: DomId | undefined }
    /** Mirror a single node by id, without its children. */
    | { type: "RequestNode"; content: DomId }
    /** Snapshot a subtree (recursive, with pruning) by id, or the whole watch scope when `undefined`. */
    | { type: "RequestSnapshot"; content: DomId | undefined }
    /** Search `from`'s descendants for nodes whose name contains `query`, mirroring the matches and their ancestors. */
    | { type: "Search"; content: SearchQuery }
    /** Mirror several single nodes by id, in one patch. */
    | { type: "RequestNodes"; content: DomId[] }
    /** Relay an upstream operation for the client to apply against the real game. */
    | { type: "Operation"; content: OperationRequest }
    /** Answer to `ClientMessage::ListSessions` (control connections only). */
    | { type: "Sessions"; content: SessionInfo[] }
    
    /** Control messages */
    
    /** Control-only: a new session was added, described in full so it can be named on sight. */
    | { type: "NewSession"; content: SessionInfo }
    /** Control-only: a session was removed. */
    | { type: "RemoveSession"; content: SessionId }
    /** Control-only: a batch of console output relayed from the given session. */
    | { type: "SessionLog"; content: SessionLogBatch }
    /** Control-only: the outcome of a `ClientMessage::RunCode` we sent. */
    | { type: "RunResult"; content: RunOutcome };

/** A struct variant, so the fields land on the wire reversed. */
const operationRequest: SerDes<OperationRequest> = {
    ser(cursor, value) {
        operation.ser(cursor, value.op);
        u32.ser(cursor, value.id);
    },

    des(cursor) {
        const id = u32.des(cursor);
        return { id, op: operation.des(cursor) };
    },
};

/** A struct variant, so the fields land on the wire reversed. */
const searchQuery: SerDes<SearchQuery> = {
    ser(cursor, value) {
        str.ser(cursor, value.query);
        str.ser(cursor, value.from);
    },

    des(cursor) {
        const from = str.des(cursor);
        return { from, query: str.des(cursor) };
    },
};

/** A struct variant, so the fields land on the wire reversed. */
const sessionLogBatch: SerDes<SessionLogBatch> = {
    ser(cursor, batch) {
        logEntries.ser(cursor, batch.entries);
        u32.ser(cursor, batch.id);
    },

    des(cursor) {
        const id = u32.des(cursor);
        return { id, entries: logEntries.des(cursor) };
    },
};

/** A struct variant, so the fields land on the wire reversed. */
const runOutcome: SerDes<RunOutcome> = {
    ser(cursor, value) {
        opResult.ser(cursor, value.result);
        u32.ser(cursor, value.request);
        u32.ser(cursor, value.session);
    },

    des(cursor) {
        const session = u32.des(cursor);
        const request = u32.des(cursor);
        return { session, request, result: opResult.des(cursor) };
    },
};

/** A plain struct, so `ser` runs forward and `des` in reverse. `SessionId` is a newtype over `u32`. */
const sessionInfo: SerDes<SessionInfo> = {
    ser(cursor, info) {
        u32.ser(cursor, info.id);
        optStr.ser(cursor, info.username);
        str.ser(cursor, info.peer);
        boolean.ser(cursor, info.active);
        u8.ser(cursor, info.securityLevel);
    },

    des(cursor) {
        const securityLevel = u8.des(cursor);
        const active = boolean.des(cursor);
        const peer = str.des(cursor);
        const username = optStr.des(cursor);
        const id = u32.des(cursor);
        return { id, username, peer, active, securityLevel };
    },
};

/** Mirrors `ServerMessage`. */
export const serverMessage: SerDes<ServerMessage> = taggedUnion<ServerMessage>([
    { type: "Hello" },
    { type: "UpstreamChanged", content: upstreamState },
    { type: "RequestChildren", content: optStr },
    { type: "RequestNode", content: str },
    { type: "RequestSnapshot", content: optStr },
    { type: "Search", content: searchQuery },
    { type: "RequestNodes", content: strArray },
    { type: "Operation", content: operationRequest },
    { type: "Sessions", content: serdeArray(sessionInfo) },
    { type: "NewSession", content: sessionInfo },
    { type: "RemoveSession", content: u32 },
    { type: "SessionLog", content: sessionLogBatch },
    { type: "RunResult", content: runOutcome },
]);

/** Decode a base64 text frame from the broker, throwing on a malformed or unknown frame. */
export function decodeServer(frame: string): ServerMessage {
    return fromBytes(serverMessage, textDecode(frame));
}

/** Encode a [`ServerMessage`] into a base64 text frame — the broker's own direction, mirrored here so a frame can be round-tripped. */
export function encodeServer(message: ServerMessage): string {
    return textEncode(toBytes(serverMessage, message));
}
