import { ComponentChildren, createContext } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { api, Result } from '../lib/fetch.ts';
import pMap from 'p-map';
import { Setter } from '../lib/ts.ts';

export interface Production {
  byA: Record<number, number[]>;
}

export const ProductionContext = createContext<Production>(
  null as unknown as Production,
);

export function WatchProduction({ children }: { children: ComponentChildren }) {
  // TODO: watch olo
  const [prod, setProd] = useState<Result<Production>>();
  const [progress, setProgress] = useState<number>(0);
  useEffect(() => void fetchProduction(setProd, setProgress), []);
  if (!prod) {
    return <p>loading production {Math.round(progress * 100)}%...</p>;
  }
  if (prod.error) {
    return <p>load error: {prod.error.message}</p>;
  }
  return (
    <ProductionContext.Provider value={prod.value}>
      {children}
    </ProductionContext.Provider>
  );
}

async function fetchProduction(
  setProd: Setter<Result<Production> | undefined>,
  setProgress: Setter<number>,
) {
  try {
    const available: number[] = await (
      await fetch(api('/api/available/ticks'))
    ).json();
    available.sort((a, b) => a - b);
    setProgress(0.05);
    const files = await pMap(
      available.slice(1000).filter((v, i) => i % 4 === 0),
      async (tick) => {
        const file: Record<number, number> = await (
          await fetch(api(`/script-output/production-${tick}.json`))
        ).json();
        setProgress((v) => v + 0.9 / available.length);
        return [tick, file] as const;
      },
      { concurrency: 8 },
    );
    setProgress(0.95);

    const byA: Record<number, number[]> = {};
    for (const [, file] of files) {
      for (const [aStr, count] of Object.entries(file)) {
        const a = Number(aStr);
        byA[a] ??= [];
        byA[a].push(count);
      }
    }
    // TODO: normalise length?
    setProd({ value: { byA }, error: undefined });
  } catch (err) {
    setProd({
      error: err instanceof Error ? err : new Error('unknown error'),
      value: undefined,
    });
  }
}
