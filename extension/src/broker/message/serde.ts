import { Squash, type SerDes } from "squash-ts";

// Squash's cursor consumes from the tail, so serde enum variants and struct
// variants land on the wire *reversed* while plain structs land in declaration
// order. Every layout here mirrors a pinned wire test in `broker/src/protocol`.

// Roblox strings can hold arbitrary bytes, so the Luau client sanitises them
// before writing; a JS string is always well-formed once `TextEncoder` is done
// with it, so `Squash.string()` is used directly.
export const str = Squash.string();
export const boolean = Squash.boolean();
export const f32 = Squash.f32();
export const f64 = Squash.f64();
export const i32 = Squash.i32();
export const i64 = Squash.i64();
export const u8 = Squash.u8();
export const u16 = Squash.u16();
export const u32 = Squash.u32();
export const rawBuffer = Squash.buffer();
export const vlq = Squash.vlq();
export const optStr = Squash.opt(str);

/** A `Vec<T>`: elements reversed, then a VLQ count — unlike `Squash.array`, which writes them forward. */
export function serdeArray<T>(serdes: SerDes<T>): SerDes<T[]> {
    return {
        ser(cursor, values) {
            for (let i = values.length - 1; i >= 0; i--) {
                serdes.ser(cursor, values[i]!);
            }
            vlq.ser(cursor, values.length);
        },

        des(cursor) {
            const count = vlq.des(cursor);
            const values: T[] = new Array<T>(count);
            for (let i = 0; i < count; i++) {
                values[i] = serdes.des(cursor);
            }
            return values;
        },
    };
}

export const strArray = serdeArray(str);

/** A tagged-union member: `content` is omitted for unit variants. */
export interface VariantSpec {
    type: string;
    /** `any` because each variant's payload type is checked at the `taggedUnion` call site, not here. */
    content?: SerDes<any>;
}

/** The shape every ported enum uses: a discriminant plus an optional payload. */
export interface Tagged {
    type: string;
    content?: unknown;
}

/**
 * A `SerDes` for a Rust enum: the payload first, then the variant's index as a
 * `u8` tag. `variants` must be listed in the Rust declaration order.
 */
export function taggedUnion<V extends Tagged>(variants: readonly VariantSpec[]): SerDes<V> {
    if (variants.length > 256) {
        throw new Error(`a tagged union holds at most 256 variants (the tag is one byte), got ${variants.length}`);
    }

    const indexByType = new Map<string, number>();
    variants.forEach((variant, index) => indexByType.set(variant.type, index));

    return {
        ser(cursor, value) {
            const index = indexByType.get(value.type);
            if (index === undefined) {
                throw new Error(`unknown variant ${value.type}`);
            }
            variants[index]!.content?.ser(cursor, value.content);
            u8.ser(cursor, index);
        },

        des(cursor) {
            const index = u8.des(cursor);
            const variant = variants[index];
            if (!variant) {
                throw new Error(`unknown variant tag ${index}`);
            }
            const decoded: Tagged = { type: variant.type };
            if (variant.content) {
                decoded.content = variant.content.des(cursor);
            }
            return decoded as V;
        },
    };
}
