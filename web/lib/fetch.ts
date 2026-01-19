import { Dispatch } from 'preact/hooks';
import { serializeError } from 'serialize-error';

export type Result<T> =
  | { value: T; error: undefined }
  | { value: undefined; error: Error };

export function fetchJson<T>(path: string, set: Dispatch<Result<T>>) {
  void fetchJsonInt(path, set);
}

class StatusError extends Error {
  readonly code: number;
  readonly path: string;
  constructor(path: string, code: number) {
    super('status code failed');
    this.code = code;
    this.path = path;
  }
}

export async function fetchJsonInt<T>(
  path: string,
  set: Dispatch<Result<T>>,
): Promise<void> {
  try {
    const resp = await fetch(api(path));
    if (!resp.ok) {
      return set({
        error: new StatusError(path, resp.status),
        value: undefined,
      });
    }
    const json = await resp.json();
    return set({ value: json, error: undefined });
  } catch (err) {
    return set({
      error:
        err instanceof Error
          ? err
          : new Error(`unknown error: ${serializeError(err)}`),
      value: undefined,
    });
  }
}

export function api(rel: string): string {
  return (import.meta.env.VITE_API_SERVER ?? '') + rel;
}
