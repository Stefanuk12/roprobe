import type { SerDes } from "squash-ts";
import { optStr, serdeArray, str, strArray, taggedUnion, u32 } from "./serde";
import { domValue, type DomId, type DomValue } from "./variant";

/** One operation the client applies against the live game. */
export type Operation =
    | { type: "Rename"; content: { node: DomId; name: string } }
    | { type: "Delete"; content: { node: DomId } }
    | { type: "Move"; content: { node: DomId; parent: DomId | undefined } }
    | { type: "Create"; content: { parent: DomId; class: string } }
    | { type: "AddTag"; content: { node: DomId; tag: string } }
    | { type: "RemoveTag"; content: { node: DomId; tag: string } }
    | { type: "SetProperty"; content: { node: DomId; name: string; value: DomValue } }
    | { type: "SetAttribute"; content: { node: DomId; name: string; value: DomValue } }
    | { type: "RemoveAttribute"; content: { node: DomId; name: string } }
    | { type: "RenameAttribute"; content: { node: DomId; old: string; new: string } }
    /** `properties` is the exact list of property names to read, filled from the API dump; empty when the class is unknown, so the client falls back to its own enumeration. */
    | { type: "GetProperties"; content: { node: DomId; properties: string[] } }
    | { type: "RunCode"; content: { source: string } };

/** One `name -> value` read. */
export interface NamedValue {
    name: string;
    value: DomValue;
}

/** A node's raw properties, tags, and attributes. */
export interface Reads {
    properties: NamedValue[];
    tags: string[];
    attributes: NamedValue[];
}

/** The outcome the client reports for a relayed [`Operation`]. */
export type OpResult =
    | { type: "Ok" }
    | { type: "Reads"; content: Reads }
    | { type: "Err"; content: string }
    | { type: "Output"; content: string };

/** One `{ name, value }` item of an [`EnumFamily`]. */
export interface EnumEntry {
    name: string;
    value: number;
}

/** One `Enum` family, so the broker has a record of a session's possible enums. */
export interface EnumFamily {
    name: string;
    items: EnumEntry[];
}

// Operation payloads are struct variants, so their fields land on the wire reversed.

/** A single-string payload keyed by `key` (e.g. `{ node }` / `{ source }`). */
function singleString<K extends string>(key: K): SerDes<Record<K, string>> {
    return {
        ser(cursor, value) {
            str.ser(cursor, value[key]);
        },

        des(cursor) {
            return { [key]: str.des(cursor) } as Record<K, string>;
        },
    };
}

/** A `{ node, [key] }` payload of two strings. */
function nodeAnd<K extends string>(key: K): SerDes<{ node: DomId } & Record<K, string>> {
    return {
        ser(cursor, value) {
            str.ser(cursor, value[key]);
            str.ser(cursor, value.node);
        },

        des(cursor) {
            const node = str.des(cursor);
            return { node, [key]: str.des(cursor) } as { node: DomId } & Record<K, string>;
        },
    };
}

const moveContent: SerDes<{ node: DomId; parent: DomId | undefined }> = {
    ser(cursor, value) {
        optStr.ser(cursor, value.parent);
        str.ser(cursor, value.node);
    },

    des(cursor) {
        const node = str.des(cursor);
        return { node, parent: optStr.des(cursor) };
    },
};

const createContent: SerDes<{ parent: DomId; class: string }> = {
    ser(cursor, value) {
        str.ser(cursor, value.class);
        str.ser(cursor, value.parent);
    },

    des(cursor) {
        const parent = str.des(cursor);
        return { parent, class: str.des(cursor) };
    },
};

/** Mirrors `SetProperty`/`SetAttribute`'s `{ node, name, value }`. */
const setValueContent: SerDes<{ node: DomId; name: string; value: DomValue }> = {
    ser(cursor, value) {
        domValue.ser(cursor, value.value);
        str.ser(cursor, value.name);
        str.ser(cursor, value.node);
    },

    des(cursor) {
        const node = str.des(cursor);
        const name = str.des(cursor);
        return { node, name, value: domValue.des(cursor) };
    },
};

/** Mirrors `GetProperties { node, properties }` — the broker fills `properties`. */
const getPropertiesContent: SerDes<{ node: DomId; properties: string[] }> = {
    ser(cursor, value) {
        strArray.ser(cursor, value.properties);
        str.ser(cursor, value.node);
    },

    des(cursor) {
        const node = str.des(cursor);
        return { node, properties: strArray.des(cursor) };
    },
};

const renameAttributeContent: SerDes<{ node: DomId; old: string; new: string }> = {
    ser(cursor, value) {
        str.ser(cursor, value.new);
        str.ser(cursor, value.old);
        str.ser(cursor, value.node);
    },

    des(cursor) {
        const node = str.des(cursor);
        const old = str.des(cursor);
        return { node, old, new: str.des(cursor) };
    },
};

/** Mirrors `Operation`. */
export const operation: SerDes<Operation> = taggedUnion<Operation>([
    { type: "Rename", content: nodeAnd("name") },
    { type: "Delete", content: singleString("node") },
    { type: "Move", content: moveContent },
    { type: "Create", content: createContent },
    { type: "AddTag", content: nodeAnd("tag") },
    { type: "RemoveTag", content: nodeAnd("tag") },
    { type: "SetProperty", content: setValueContent },
    { type: "SetAttribute", content: setValueContent },
    { type: "RemoveAttribute", content: nodeAnd("name") },
    { type: "RenameAttribute", content: renameAttributeContent },
    { type: "GetProperties", content: getPropertiesContent },
    { type: "RunCode", content: singleString("source") },
]);

// Result structures are plain structs: `ser` forward, `des` reversed.

const namedValue: SerDes<NamedValue> = {
    ser(cursor, pair) {
        str.ser(cursor, pair.name);
        domValue.ser(cursor, pair.value);
    },

    des(cursor) {
        const value = domValue.des(cursor);
        return { name: str.des(cursor), value };
    },
};

const namedValues = serdeArray(namedValue);

const reads: SerDes<Reads> = {
    ser(cursor, value) {
        namedValues.ser(cursor, value.properties);
        strArray.ser(cursor, value.tags);
        namedValues.ser(cursor, value.attributes);
    },

    des(cursor) {
        const attributes = namedValues.des(cursor);
        const tags = strArray.des(cursor);
        return { properties: namedValues.des(cursor), tags, attributes };
    },
};

/** Mirrors `OpResult`. */
export const opResult: SerDes<OpResult> = taggedUnion<OpResult>([
    { type: "Ok" },
    { type: "Reads", content: reads },
    { type: "Err", content: str },
    { type: "Output", content: str },
]);

const enumEntry: SerDes<EnumEntry> = {
    ser(cursor, entry) {
        str.ser(cursor, entry.name);
        u32.ser(cursor, entry.value);
    },

    des(cursor) {
        const value = u32.des(cursor);
        return { name: str.des(cursor), value };
    },
};

const enumEntries = serdeArray(enumEntry);

const enumFamily: SerDes<EnumFamily> = {
    ser(cursor, family) {
        str.ser(cursor, family.name);
        enumEntries.ser(cursor, family.items);
    },

    des(cursor) {
        const items = enumEntries.des(cursor);
        return { name: str.des(cursor), items };
    },
};

/** Mirrors `Vec<EnumFamily>` — the `EnumFamilies` message payload. */
export const enumFamilies = serdeArray(enumFamily);
