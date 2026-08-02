import { Squash, type SerDes } from "squash-ts";
import { boolean } from "./serde";

/** An external tool the broker bridges to. */
export type Upstream = "Verde" | "LuauLsp";

/** One connected client session's id, unique for the broker's lifetime. */
export type SessionId = number;

/** The `{ upstream, enabled }` payload shared by `SetUpstream` and `UpstreamChanged`. */
export interface UpstreamState {
    upstream: Upstream;
    enabled: boolean;
}

/** Mirrors `Upstream`. */
export const upstream: SerDes<Upstream> = Squash.literal<Upstream>("Verde", "LuauLsp");

/** A struct variant, so the fields land on the wire reversed. */
export const upstreamState: SerDes<UpstreamState> = {
    ser(cursor, state) {
        boolean.ser(cursor, state.enabled);
        upstream.ser(cursor, state.upstream);
    },

    des(cursor) {
        const name = upstream.des(cursor);
        return { upstream: name, enabled: boolean.des(cursor) };
    },
};
