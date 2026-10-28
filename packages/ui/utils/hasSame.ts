export const hasSameKeys = (
  a: Record<string, unknown>,
  b: Record<string, unknown>,
): boolean => {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  const set = new Set(keysB);
  return keysA.every((k) => set.has(k));
};

export const hasSameItems = <T>(a: T[], b: T[]): boolean => {
  if (a.length !== b.length) return false;
  const set = new Set(b);
  return a.every((item) => set.has(item));
};
