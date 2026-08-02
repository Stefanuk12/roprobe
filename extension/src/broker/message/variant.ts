import { Squash, type SerDes } from "squash-ts";
import type * as RT from "squash-ts";
import {
    boolean,
    f32,
    f64,
    i32,
    i64,
    optStr,
    rawBuffer,
    serdeArray,
    str,
    taggedUnion,
    u16,
    u32,
    u8,
} from "./serde";

/** The client's identity for an instance (`Instance:GetDebugId()`), opaque to the broker. */
export type DomId = string;

/** A `Content`'s source, mirroring `rbx_types::ContentType`. */
export type ContentValue =
    | { type: "None" }
    | { type: "Uri"; content: string | undefined }
    | { type: "Object"; content: DomId };

/** A `CFrame`'s 12 `GetComponents()` values: `x, y, z` then the rotation matrix row-major. */
export type CFrameComponents = [
    number, number, number,
    number, number, number,
    number, number, number,
    number, number, number,
];

/** A `PhysicalProperties` including `AcousticAbsorption`, which Squash's own codec omits. */
export interface PhysicalPropertiesValue {
    Density: number;
    Friction: number;
    Elasticity: number;
    FrictionWeight: number;
    ElasticityWeight: number;
    AcousticAbsorption: number;
}

/** A `Font` as the broker carries it: a family string plus numeric `FontWeight`/`FontStyle` ids. */
export interface FontValue {
    Family: string;
    Weight: number;
    Style: number;
}

/** An `EnumItem` as its family's index into the client's `Enum:GetEnums()` catalog plus its raw value. */
export interface EnumValue {
    family: number;
    value: number;
}

/** A subset of `rbx_types::Variant`, specialised for the binary format. */
export type DomValue =
    | { type: "Bool"; content: boolean }
    | { type: "Float"; content: number }
    | { type: "Int"; content: number }
    | { type: "String"; content: string }
    | { type: "Ref"; content: DomId }
    | { type: "Enum"; content: EnumValue }
    | { type: "Vector2"; content: RT.Vector2 }
    | { type: "Vector3"; content: RT.Vector3 }
    | { type: "Color3"; content: RT.Color3 }
    | { type: "UDim"; content: RT.UDim }
    | { type: "UDim2"; content: RT.UDim2 }
    | { type: "NumberRange"; content: RT.NumberRange }
    | { type: "Rect"; content: RT.Rect }
    | { type: "BrickColor"; content: RT.BrickColor }
    | { type: "CFrame"; content: CFrameComponents }
    | { type: "Float32"; content: number }
    | { type: "Int32"; content: number }
    | { type: "ContentId"; content: string }
    | { type: "BinaryString"; content: Uint8Array }
    | { type: "Color3uint8"; content: RT.Color3 }
    | { type: "Vector2int16"; content: RT.Vector2int16 }
    | { type: "Vector3int16"; content: RT.Vector3int16 }
    | { type: "Ray"; content: RT.Ray }
    | { type: "Region3"; content: RT.Region3 }
    | { type: "Region3int16"; content: RT.Region3int16 }
    | { type: "Axes"; content: RT.Axes }
    | { type: "Faces"; content: RT.Faces }
    | { type: "Font"; content: FontValue }
    | { type: "NumberSequence"; content: RT.NumberSequenceKeypoint[] }
    | { type: "ColorSequence"; content: RT.ColorSequenceKeypoint[] }
    | { type: "PhysicalProperties"; content: PhysicalPropertiesValue | undefined }
    | { type: "OptionalCFrame"; content: CFrameComponents | undefined }
    | { type: "Content"; content: ContentValue };

/** Mirrors `ContentValue`, whose `Uri` payload is itself optional. */
const contentValue: SerDes<ContentValue> = taggedUnion<ContentValue>([
    { type: "None" },
    { type: "Uri", content: optStr },
    { type: "Object", content: str },
]);

/** Mirrors `Enum(familyIndex, value)` — a tuple variant, so the value lands first. */
const enumValue: SerDes<EnumValue> = {
    ser(cursor, value) {
        u32.ser(cursor, value.value);
        u16.ser(cursor, value.family);
    },

    des(cursor) {
        const family = u16.des(cursor);
        return { family, value: u32.des(cursor) };
    },
};

const CFRAME_COMPONENTS = 12;
const cframeValue: SerDes<CFrameComponents> = {
    ser(cursor, components) {
        for (let i = CFRAME_COMPONENTS - 1; i >= 0; i--) {
            f32.ser(cursor, components[i]!);
        }
    },

    des(cursor) {
        const components = new Array<number>(CFRAME_COMPONENTS);
        for (let i = 0; i < CFRAME_COMPONENTS; i++) {
            components[i] = f32.des(cursor);
        }
        return components as CFrameComponents;
    },
};

/** Mirrors `Option<[f32; 6]>` — an array, so the fields land reversed. */
const physicalPropertiesValue: SerDes<PhysicalPropertiesValue> = {
    ser(cursor, props) {
        f32.ser(cursor, props.AcousticAbsorption);
        f32.ser(cursor, props.ElasticityWeight);
        f32.ser(cursor, props.FrictionWeight);
        f32.ser(cursor, props.Elasticity);
        f32.ser(cursor, props.Friction);
        f32.ser(cursor, props.Density);
    },

    des(cursor) {
        const Density = f32.des(cursor);
        const Friction = f32.des(cursor);
        const Elasticity = f32.des(cursor);
        const FrictionWeight = f32.des(cursor);
        const ElasticityWeight = f32.des(cursor);
        return {
            Density,
            Friction,
            Elasticity,
            FrictionWeight,
            ElasticityWeight,
            AcousticAbsorption: f32.des(cursor),
        };
    },
};

/** Mirrors `Font(family, weight, style)` — a tuple variant, so the style lands first. */
const fontValue: SerDes<FontValue> = {
    ser(cursor, font) {
        u8.ser(cursor, font.Style);
        u16.ser(cursor, font.Weight);
        str.ser(cursor, font.Family);
    },

    des(cursor) {
        const Family = str.des(cursor);
        const Weight = u16.des(cursor);
        return { Family, Weight, Style: u8.des(cursor) };
    },
};

/** Mirrors `DomValue`. */
export const domValue: SerDes<DomValue> = taggedUnion<DomValue>([
    { type: "Bool", content: boolean },
    { type: "Float", content: f64 },
    { type: "Int", content: i64 },
    { type: "String", content: str },
    { type: "Ref", content: str },
    { type: "Enum", content: enumValue },
    { type: "Vector2", content: Squash.Vector2(f32) },
    { type: "Vector3", content: Squash.Vector3(f32) },
    { type: "Color3", content: Squash.Color3() },
    { type: "UDim", content: Squash.UDim(f32) },
    { type: "UDim2", content: Squash.UDim2(f32) },
    { type: "NumberRange", content: Squash.NumberRange(f32) },
    { type: "Rect", content: Squash.Rect(f32) },
    { type: "BrickColor", content: Squash.BrickColor() },
    { type: "CFrame", content: cframeValue },
    { type: "Float32", content: f32 },
    { type: "Int32", content: i32 },
    { type: "ContentId", content: str },
    { type: "BinaryString", content: rawBuffer },
    { type: "Color3uint8", content: Squash.Color3() },
    { type: "Vector2int16", content: Squash.Vector2int16() },
    { type: "Vector3int16", content: Squash.Vector3int16() },
    { type: "Ray", content: Squash.Ray(f32) },
    { type: "Region3", content: Squash.Region3(f32) },
    { type: "Region3int16", content: Squash.Region3int16() },
    { type: "Axes", content: Squash.Axes() },
    { type: "Faces", content: Squash.Faces() },
    { type: "Font", content: fontValue },
    { type: "NumberSequence", content: serdeArray(Squash.NumberSequenceKeypoint(f32)) },
    { type: "ColorSequence", content: serdeArray(Squash.ColorSequenceKeypoint()) },
    { type: "PhysicalProperties", content: Squash.opt(physicalPropertiesValue) },
    { type: "OptionalCFrame", content: Squash.opt(cframeValue) },
    { type: "Content", content: contentValue },
]);
