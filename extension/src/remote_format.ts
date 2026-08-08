// Renders a captured call back into the Luau that reproduces it, for the detail
// view and for pasting into an executor. A value the wire cannot carry (a
// function, a thread) renders as a commented placeholder.
import type { DomValue, LuaEntry, LuaTable, LuaValue, RemoteCall } from "./broker/message";

const LUAU_IDENTIFIER = /^[A-Za-z_][A-Za-z0-9_]*$/;
const LUAU_KEYWORDS = new Set([
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    "continue", "export", "type",
]);

const ESCAPES: Record<number, string> = {
    0x07: "\\a", 0x08: "\\b", 0x09: "\\t", 0x0a: "\\n",
    0x0b: "\\v", 0x0c: "\\f", 0x0d: "\\r", 0x22: "\\\"", 0x5c: "\\\\",
};

/** Whether `name` can be written as `t.name` rather than `t["name"]`. */
function isBareKey(name: string): boolean {
    return LUAU_IDENTIFIER.test(name) && !LUAU_KEYWORDS.has(name);
}

/**
 * A Luau string literal for arbitrary bytes. Remote payloads are routinely
 * packed binary, so anything outside printable ASCII becomes a `\ddd` escape,
 * which round-trips byte for byte where a UTF-8 decode would not.
 */
export function luauString(bytes: Uint8Array): string {
    let out = "\"";
    for (let index = 0; index < bytes.length; index += 1) {
        const byte = bytes[index];
        const escape = ESCAPES[byte];
        if (escape !== undefined) {
            out += escape;
        } else if (byte >= 0x20 && byte <= 0x7e) {
            out += String.fromCharCode(byte);
        } else {
            // Luau reads up to three digits after the backslash, so an escape in
            // front of an ASCII digit is padded out or the two would merge.
            const next = bytes[index + 1];
            const width = next !== undefined && next >= 0x30 && next <= 0x39 ? 3 : 1;
            out += `\\${String(byte).padStart(width, "0")}`;
        }
    }
    return `${out}"`;
}

/** A Luau number literal, keeping the special values Luau spells out in words. */
function luauNumber(value: number): string {
    if (Number.isNaN(value)) {
        return "0/0";
    }
    if (value === Infinity) {
        return "math.huge";
    }
    if (value === -Infinity) {
        return "-math.huge";
    }
    return String(value);
}

/** A Roblox datatype as the constructor call that rebuilds it. */
function luauDatatype(value: DomValue): string {
    switch (value.type) {
        case "Bool":
            return String(value.content);
        case "Float":
        case "Int":
        case "Float32":
        case "Int32":
            return luauNumber(value.content);
        case "String":
        case "ContentId":
            return luauString(new TextEncoder().encode(value.content));
        case "BinaryString":
            return luauString(value.content);
        case "Ref":
            return `--[[ ref ${value.content} ]] nil`;
        case "Enum":
            // The family is an index into the client's catalog, so only the raw
            // value survives the trip.
            return `--[[ Enum family ${value.content.family} ]] ${value.content.value}`;
        case "Vector2":
            return `Vector2.new(${value.content.X}, ${value.content.Y})`;
        case "Vector2int16":
            return `Vector2int16.new(${value.content.X}, ${value.content.Y})`;
        case "Vector3":
            return `Vector3.new(${value.content.X}, ${value.content.Y}, ${value.content.Z})`;
        case "Vector3int16":
            return `Vector3int16.new(${value.content.X}, ${value.content.Y}, ${value.content.Z})`;
        case "Color3":
        case "Color3uint8":
            return `Color3.new(${value.content.R}, ${value.content.G}, ${value.content.B})`;
        case "UDim":
            return `UDim.new(${value.content.Scale}, ${value.content.Offset})`;
        case "UDim2":
            return (
                `UDim2.new(${value.content.X.Scale}, ${value.content.X.Offset}, ` +
                `${value.content.Y.Scale}, ${value.content.Y.Offset})`
            );
        case "NumberRange":
            return `NumberRange.new(${value.content.Min}, ${value.content.Max})`;
        case "Rect":
            return (
                `Rect.new(${value.content.Min.X}, ${value.content.Min.Y}, ` +
                `${value.content.Max.X}, ${value.content.Max.Y})`
            );
        case "BrickColor":
            return `BrickColor.new(${value.content.Number})`;
        case "CFrame":
            return `CFrame.new(${value.content.join(", ")})`;
        case "OptionalCFrame":
            return value.content ? `CFrame.new(${value.content.join(", ")})` : "nil";
        case "Ray":
            return (
                `Ray.new(Vector3.new(${value.content.Origin.X}, ${value.content.Origin.Y}, ${value.content.Origin.Z}), ` +
                `Vector3.new(${value.content.Direction.X}, ${value.content.Direction.Y}, ${value.content.Direction.Z}))`
            );
        case "Region3":
            return `--[[ Region3 at ${value.content.Position.X}, ${value.content.Position.Y}, ${value.content.Position.Z} ]] nil`;
        case "Region3int16":
            return "--[[ Region3int16 ]] nil";
        case "Axes":
        case "Faces":
            return `--[[ ${value.type} ]] nil`;
        case "Font":
            return `Font.new(${luauString(new TextEncoder().encode(value.content.Family))}, ` +
                `${value.content.Weight}, ${value.content.Style})`;
        case "NumberSequence":
            return `NumberSequence.new({ ${value.content
                .map((k) => `NumberSequenceKeypoint.new(${k.Time}, ${k.Value}, ${k.Envelope})`)
                .join(", ")} })`;
        case "ColorSequence":
            return `ColorSequence.new({ ${value.content
                .map((k) => `ColorSequenceKeypoint.new(${k.Time}, Color3.new(${k.Value.R}, ${k.Value.G}, ${k.Value.B}))`)
                .join(", ")} })`;
        case "PhysicalProperties":
            return value.content
                ? `PhysicalProperties.new(${value.content.Density}, ${value.content.Friction}, ` +
                      `${value.content.Elasticity}, ${value.content.FrictionWeight}, ${value.content.ElasticityWeight})`
                : "nil";
        case "Content":
            return value.content.type === "Uri" && value.content.content !== undefined
                ? `Content.fromUri(${luauString(new TextEncoder().encode(value.content.content))})`
                : `--[[ Content ${value.content.type} ]] nil`;
    }
}

/** The path expression that finds an instance again, service-rooted where it can. */
export function instancePath(path: string): string {
    const parts = path.split(".");
    const service = parts.shift();
    if (service === undefined || service === "") {
        return "nil";
    }
    // A lone segment is a name with no parent chain, so it finds nothing;
    // rooting it at `game:GetService` would build a call that throws.
    if (parts.length === 0) {
        return `--[[ ${service} ]] nil`;
    }

    const rest = parts
        .map((part) => (isBareKey(part) ? `.${part}` : `[${luauString(new TextEncoder().encode(part))}]`))
        .join("");
    return `game:GetService(${luauString(new TextEncoder().encode(service))})${rest}`;
}

/** Every table in a call, by the id a `Cycle` points back at. */
function indexTables(values: LuaValue[], into = new Map<number, LuaTable>()): Map<number, LuaTable> {
    for (const value of values) {
        if (value.type !== "Table") {
            continue;
        }
        into.set(value.content.id, value.content);
        indexTables(value.content.array, into);
        indexTables(value.content.entries.flatMap((entry: LuaEntry) => [entry.key, entry.value]), into);
    }
    return into;
}

function formatTable(table: LuaTable, indent: string, tables: Map<number, LuaTable>): string {
    const inner = `${indent}    `;
    const lines: string[] = [];

    for (const value of table.array) {
        lines.push(`${inner}${format(value, inner, tables)},`);
    }
    for (const { key, value } of table.entries) {
        const rendered = format(value, inner, tables);
        // A plain string key is the common case and reads far better bare.
        if (key.type === "String") {
            const name = new TextDecoder().decode(key.content);
            lines.push(`${inner}${isBareKey(name) ? name : `[${luauString(key.content)}]`} = ${rendered},`);
        } else {
            lines.push(`${inner}[${format(key, inner, tables)}] = ${rendered},`);
        }
    }
    if (table.truncated) {
        lines.push(`${inner}-- truncated: the capture caps cut this table short`);
    }
    if (table.metatable) {
        lines.push(`${inner}-- this table carried a metatable, which is not captured`);
    }

    if (lines.length === 0) {
        return "{}";
    }
    return `{\n${lines.join("\n")}\n${indent}}`;
}

/** One captured value as the Luau that rebuilds it. */
export function format(value: LuaValue, indent: string, tables: Map<number, LuaTable>): string {
    switch (value.type) {
        case "Nil":
            return "nil";
        case "Bool":
            return String(value.content);
        case "Number":
            return luauNumber(value.content);
        case "String":
            return luauString(value.content);
        case "Buffer":
            return `buffer.fromstring(${luauString(value.content)})`;
        case "Table":
            return formatTable(value.content, indent, tables);
        case "Instance":
            return instancePath(value.content.path);
        case "Roblox":
            return luauDatatype(value.content);
        case "Function": {
            const where = [value.content.chunk, value.content.line].filter((part) => part !== undefined).join(":");
            const name = value.content.name ?? "anonymous";
            return `--[[ function ${name}${where ? ` @ ${where}` : ""} ]] nil`;
        }
        case "Cycle":
            return tables.has(value.content)
                ? `--[[ cyclic reference to table #${value.content} ]] nil`
                : "--[[ cyclic reference ]] nil";
        case "Opaque":
            return `--[[ ${value.content} ]] nil`;
    }
}

/** A call's arguments as an argument list, wrapping once any of them is multi-line. */
function formatArguments(values: LuaValue[], tables: Map<number, LuaTable>): string {
    if (values.length === 0) {
        return "()";
    }

    const inline = values.map((value) => format(value, "", tables));
    const joined = inline.join(", ");
    if (!joined.includes("\n") && joined.length <= 100) {
        return `(${joined})`;
    }

    const wrapped = values.map((value) => `    ${format(value, "    ", tables)}`);
    return `(\n${wrapped.join(",\n")}\n)`;
}

/** The Luau that reproduces a captured call, ready to paste into an executor. */
export function callSnippet(call: RemoteCall): string {
    const tables = indexTables(call.arguments);
    if (call.returns) {
        indexTables(call.returns, tables);
    }

    const target = instancePath(call.remote.path);
    const args = formatArguments(call.arguments, tables);

    // An incoming call is not something you can make; show what arrived instead.
    if (call.direction === "Incoming") {
        return `-- ${call.remote.class}.${call.method} delivered:\nlocal arguments = table.pack${args}`;
    }
    return `${target}:${call.method}${args}`;
}

/** A one-line summary of a call's arguments, for a tree row. */
export function summarise(call: RemoteCall): string {
    if (call.arguments.length === 0) {
        return "()";
    }

    const tables = indexTables(call.arguments);
    const parts = call.arguments.map((value) => {
        const rendered = format(value, "", tables).replace(/\s+/g, " ");
        return rendered.length > 24 ? `${rendered.slice(0, 23)}…` : rendered;
    });

    const joined = parts.join(", ");
    return `(${joined.length > 80 ? `${joined.slice(0, 79)}…` : joined})`;
}

/** The full detail view of one call: where it came from, and how to reproduce it. */
export function callDocument(call: RemoteCall, sessionLabel: string): string {
    const tables = indexTables(call.arguments);
    if (call.returns) {
        indexTables(call.returns, tables);
    }

    const arrow = call.direction === "Outgoing" ? "client -> server" : "server -> client";
    const header = [
        `-- ${call.remote.class} ${call.remote.name} (${arrow})`,
        `-- path      ${call.remote.path}`,
        `-- method    ${call.method}`,
        `-- session   ${sessionLabel}`,
        `-- captured  ${call.timestamp.toFixed(3)}s into the client's run`,
    ];

    const { script, functionName, chunk, line, isExecutor, actor } = call.source;
    if (script) {
        header.push(`-- script    ${script}`);
    }
    if (functionName || chunk || line !== undefined) {
        const where = [chunk, line].filter((part) => part !== undefined).join(":");
        header.push(`-- caller    ${functionName ?? "anonymous"}${where ? ` @ ${where}` : ""}`);
    }
    if (actor) {
        header.push(`-- actor     ${actor}`);
    }
    if (isExecutor) {
        header.push("-- NOTE      this call came from an executor thread, not the game");
    }

    const body = [header.join("\n"), "", callSnippet(call)];
    if (call.returns) {
        body.push("", "-- answered with:", `local returned = table.pack${formatArguments(call.returns, tables)}`);
    }
    return `${body.join("\n")}\n`;
}
