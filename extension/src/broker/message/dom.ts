import { Squash, type SerDes } from "squash-ts";
import { boolean, optStr, serdeArray, str, strArray, taggedUnion } from "./serde";
import { domValue, type DomId, type DomValue } from "./variant";

/** One mirrored instance. `hasChildren` reflects the *live* instance, so a tree can show an expand arrow for a node whose children aren't mirrored yet. */
export interface DomInstance {
    id: DomId;
    parent: DomId | undefined;
    class: string;
    name: string;
    hasChildren: boolean;
    properties: Map<string, DomValue>;
    attributes: Map<string, DomValue>;
    tags: string[] | undefined;
}

/** How a [`DomUpdate`] touches the stored tag set. */
export type TagChange =
    | { type: "None" }
    | { type: "Replace"; content: string[] }
    | { type: "Delta"; content: { add: string[]; remove: string[] } };

/** A lightweight change to an *already-mirrored* instance; an `undefined` map value removes the key. */
export interface DomUpdate {
    id: DomId;
    properties: Map<string, DomValue | undefined>;
    attributes: Map<string, DomValue | undefined>;
    tags?: TagChange;
}

/** A lazy, incremental update to a DOM. */
export interface DomPatch {
    /** Add an instance, or refresh it (its parent must already be mirrored). */
    upserts: DomInstance[];
    /** The ref ids of nodes to remove. */
    removals: DomId[];
    /** Property/attribute/tag-only changes to already-mirrored instances. */
    updates: DomUpdate[];
}

const valueMap = Squash.map(str, domValue);
const changeMap = Squash.map(str, Squash.opt(domValue));
const optTags = Squash.opt(strArray);

/** Mirrors `DomInstance` — a plain struct, so `ser` runs forward and `des` in reverse. */
export const domInstance: SerDes<DomInstance> = {
    ser(cursor, instance) {
        str.ser(cursor, instance.id);
        optStr.ser(cursor, instance.parent);
        str.ser(cursor, instance.class);
        str.ser(cursor, instance.name);
        boolean.ser(cursor, instance.hasChildren);
        valueMap.ser(cursor, instance.properties);
        valueMap.ser(cursor, instance.attributes);
        optTags.ser(cursor, instance.tags);
    },

    des(cursor) {
        const tags = optTags.des(cursor);
        const attributes = valueMap.des(cursor);
        const properties = valueMap.des(cursor);
        const hasChildren = boolean.des(cursor);
        const name = str.des(cursor);
        const cls = str.des(cursor);
        const parent = optStr.des(cursor);
        const id = str.des(cursor);
        return { id, parent, class: cls, name, hasChildren, properties, attributes, tags };
    },
};

/** Mirrors `TagChange::Delta` — a struct variant, so the fields land reversed. */
const tagDelta: SerDes<{ add: string[]; remove: string[] }> = {
    ser(cursor, delta) {
        strArray.ser(cursor, delta.remove);
        strArray.ser(cursor, delta.add);
    },

    des(cursor) {
        const add = strArray.des(cursor);
        const remove = strArray.des(cursor);
        return { add, remove };
    },
};

/** Mirrors `TagChange`. */
export const tagChange: SerDes<TagChange> = taggedUnion<TagChange>([
    { type: "None" },
    { type: "Replace", content: strArray },
    { type: "Delta", content: tagDelta },
]);

const TAGS_UNCHANGED: TagChange = { type: "None" };

/** Mirrors `DomUpdate`. */
export const domUpdate: SerDes<DomUpdate> = {
    ser(cursor, update) {
        str.ser(cursor, update.id);
        changeMap.ser(cursor, update.properties);
        changeMap.ser(cursor, update.attributes);
        tagChange.ser(cursor, update.tags ?? TAGS_UNCHANGED);
    },

    des(cursor) {
        const tags = tagChange.des(cursor);
        const attributes = changeMap.des(cursor);
        const properties = changeMap.des(cursor);
        const id = str.des(cursor);
        return { id, properties, attributes, tags };
    },
};

const upserts = serdeArray(domInstance);
const updates = serdeArray(domUpdate);

/** Mirrors `DomPatch`. */
export const domPatch: SerDes<DomPatch> = {
    ser(cursor, patch) {
        upserts.ser(cursor, patch.upserts);
        strArray.ser(cursor, patch.removals);
        updates.ser(cursor, patch.updates);
    },

    des(cursor) {
        const patchUpdates = updates.des(cursor);
        const removals = strArray.des(cursor);
        const patchUpserts = upserts.des(cursor);
        return { upserts: patchUpserts, removals, updates: patchUpdates };
    },
};
