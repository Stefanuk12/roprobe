import { Squash, type SerDes } from "squash-ts";
import { serdeArray, str } from "./serde";

/**
 * The severity of one relayed console line.
 */
export type LogLevel = "print" | "info" | "warn" | "error";

/** One line of a client's console output. */
export interface LogEntry {
    level: LogLevel;
    content: string;
}

/** Mirrors `LogLevel`. */
export const logLevel: SerDes<LogLevel> = Squash.literal<LogLevel>("print", "info", "warn", "error");

/** A plain struct, so `ser` runs forward and `des` in reverse. */
export const logEntry: SerDes<LogEntry> = {
    ser(cursor, entry) {
        logLevel.ser(cursor, entry.level);
        str.ser(cursor, entry.content);
    },

    des(cursor) {
        const content = str.des(cursor);
        return { level: logLevel.des(cursor), content };
    },
};

/** Mirrors `Vec<LogEntry>`. */
export const logEntries = serdeArray(logEntry);
