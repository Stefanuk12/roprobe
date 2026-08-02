import { Squash, type SerDes } from "squash-ts";

// Mirrors the broker's `protocol::transport`: frames ride WebSocket *text*
// frames as standard base64, because the target executor's socket rejects
// binary frames (closing with 1007) and UTF-8-validates text.

/** How much room a fresh encode cursor reserves; it grows past this as needed. */
const CURSOR_SIZE = 256;

/** Encode one value into a standalone Squash frame. */
export function toBytes<T>(serdes: SerDes<T>, value: T): Uint8Array {
    const cursor = Squash.cursor(CURSOR_SIZE);
    serdes.ser(cursor, value);
    return new Uint8Array(Squash.tobuffer(cursor));
}

/** Decode one value from a whole Squash frame, whose cursor starts at the tail. */
export function fromBytes<T>(serdes: SerDes<T>, frame: Uint8Array): T {
    const buffer = frame.buffer.slice(frame.byteOffset, frame.byteOffset + frame.byteLength);
    return serdes.des(Squash.frombuffer(buffer as ArrayBuffer));
}

/** Base64-encode raw bytes into a UTF-8-safe text payload. */
export function textEncode(bytes: Uint8Array): string {
    return Buffer.from(bytes).toString("base64");
}

/** Decode a base64 text payload back to raw bytes. */
export function textDecode(text: string): Uint8Array {
    return Buffer.from(text, "base64");
}
