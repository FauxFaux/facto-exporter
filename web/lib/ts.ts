export const keysOf = Object.keys as <T extends object>(
  obj: T,
) => Array<keyof T>;

export const entriesOf = Object.entries as <T extends object>(
  obj: T,
) => Array<[keyof T, T[keyof T]]>;

export const valuesOf = Object.values as <T extends object>(
  obj: T,
) => Array<T[keyof T]>;

export const minBy = <T = any>(arr: T[], fn: (v: T) => number) => {
  const mapped = arr.map(fn);
  const min = Math.min(...mapped);
  return arr[mapped.indexOf(min)];
};
