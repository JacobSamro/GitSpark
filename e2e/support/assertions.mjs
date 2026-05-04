export function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

export function shortOid(oid) {
  return oid.slice(0, 7);
}
