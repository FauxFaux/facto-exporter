import { Dispatch, StateUpdater } from 'preact/hooks';

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

export function ea<T>(v: T[] | Record<string, never> | undefined): T[] {
  if (Array.isArray(v)) {
    return v;
  }
  return [];
}

export const debounce = <F extends (...args: Parameters<F>) => ReturnType<F>>(
  func: F,
  waitFor: number,
) => {
  let timeout: NodeJS.Timeout;

  return (...args: Parameters<F>) => {
    clearTimeout(timeout);
    timeout = setTimeout(() => func(...args), waitFor);
  };
};

export type Setter<S> = Dispatch<StateUpdater<S>>;
