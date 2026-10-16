import type { Collection, Folder, Request } from "@/bindings";

/**
 * The generated bindings mark every `#[serde(default)]` field as optional (`json?: string`),
 * because Rust *accepts* the field missing when reading YAML. Values that come *from* Rust
 * always have every field, though. `Complete<T>` is the same type with all fields required,
 * derived from the generated one, so nothing is duplicated by hand.
 */
export type Complete<T> = T extends (infer U)[]
  ? Complete<U>[]
  : T extends object
    ? // `as` filters keys while keeping the mapping "homomorphic", so `-?` also removes the
      // `undefined` that optional fields carry (an `Exclude<keyof T, ...>` key list wouldn't).
      { [K in keyof T as K extends SkippedWhenEmpty ? never : K]-?: Complete<T[K]> } & {
        [K in keyof T as K extends SkippedWhenEmpty ? K : never]?: T[K];
      }
    : T;

/**
 * Fields Rust leaves out when they are empty (`skip_serializing_if`), so they can really be
 * missing in values from Rust: they stay optional in {@link Complete}. Read them with `?? ""`.
 */
type SkippedWhenEmpty = "description";

/** A request as returned by Rust: every field present. */
export type RequestModel = Complete<Request>;
export type FolderModel = Complete<Folder>;
export type CollectionModel = Complete<Collection>;

/** Marks a value received from a Rust command as complete (see {@link Complete}). */
export const fromRust = <T>(value: T) => value as Complete<T>;
