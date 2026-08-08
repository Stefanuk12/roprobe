--- The shape every tagged union on the wire takes: a discriminant plus a payload.
--- Kept apart from the codec so naming it does not drag Squash in behind it,
--- which the actor bundle would carry for a type alias.
export type EnumVariant<T, C = nil> = {
    type: T,
    content: C,
}

return true
