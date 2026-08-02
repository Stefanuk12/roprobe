// The broker wire protocol, mirroring `broker/src/protocol` module for module:
//   serde.ts     how squash-rs's derives lay values out (see the layout note there)
//   transport.ts framing — Squash bytes in and out of base64 text frames
//   variant.ts   DomValue          <- protocol/dom/variant.rs
//   dom.ts       DomInstance/Patch <- protocol/dom/{instance,update,patch}.rs
//   operation.ts Operation/OpResult<- protocol/operation.rs
//   upstream.ts  Upstream/SessionId
//   client.ts    ClientMessage     <- protocol/client.rs
//   server.ts    ServerMessage     <- protocol/server.rs

export * from "./client";
export * from "./dom";
export * from "./operation";
export * from "./server";
export * from "./upstream";
export * from "./variant";
export { serdeArray, taggedUnion, type Tagged, type VariantSpec } from "./serde";
export { fromBytes, textDecode, textEncode, toBytes } from "./transport";
