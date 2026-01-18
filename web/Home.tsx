import {
  IngredientPrototype,
  ProductPrototype,
} from 'factorio-raw-types/prototypes';
import { useEffect, useState } from 'preact/hooks';
import { fetchJson, Result } from './fetch.ts';
import { serializeError } from 'serialize-error';

interface Assemblers {
  t: Record<
    number,
    {
      surface: string;
      type: 'assembling-machine' | 'furnace';
      name: string;
      position: [number, number];
      recipe?: string;
      products_finished: number;
      direction: number;
    }
  >;
  tick: number;
  recps: Record<
    string,
    {
      ingredients: IngredientPrototype[];
      products: ProductPrototype[];
    }
  >;
}

export function Home() {
  const [ass, setAss] = useState<Result<Assemblers>>();

  useEffect(() => fetchJson('/script-output/assemblers.json', setAss), []);

  if (!ass) {
    return <h1>initial data load</h1>;
  }

  if (ass.error) {
    return <h1>erroh: {JSON.stringify(serializeError(ass.error))}</h1>;
  }

  return (
    <div>
      <p>
        {Object.values(ass.value.t).length} assemblers,{' '}
        {Object.values(ass.value.recps).length} recipies
      </p>
    </div>
  );
}
