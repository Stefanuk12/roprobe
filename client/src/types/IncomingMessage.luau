export type OperationMessage = {
    type: "operation",
    operationId: string,
    operation: { type: string, [string]: any },
}
export type RequestSnapshotMessage = {
    type: "request_snapshot",
    full: boolean?,
}
export type RequestChildrenMessage = {
    type: "request_children",
    parentIds: { string },
}
export type ReleaseSubtreeMessage = {
    type: "release_subtree",
    parentIds: { string }?,
    nodeIds: { string }?,
}
export type RequestSearchMessage = {
    type: "request_search",
    query: string?,
}
export type AckMessage = {
    type: "ack",
    requestId: string?,
}
export type IncomingMessage =
    OperationMessage
    | RequestSnapshotMessage
    | RequestChildrenMessage
    | ReleaseSubtreeMessage
    | RequestSearchMessage
    | AckMessage

return nil
