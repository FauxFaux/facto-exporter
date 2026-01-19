import {
  IngredientPrototype,
  ProductPrototype,
} from 'factorio-raw-types/prototypes';
import { ComponentChildren, createContext } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { fetchJson, Result } from '../lib/fetch.ts';
import { serializeError } from 'serialize-error';
import { keysOf } from '../lib/ts.ts';

export interface Atlas {
  assemblers: AssemblersJson['t'];
  tick: number;
  recps: AssemblersJson['recps'];
  availableImages: Record<string, [number, number][]>;
  recpName: Record<string, string>;
  entityName: Record<string, string>;
  itemName: (v: IngredientPrototype | ProductPrototype) => string;
}

export const AtlasContext = createContext<Atlas>(null as unknown as Atlas);

export function LoadAtlas({ children }: { children: ComponentChildren }) {
  const [assems, setAssems] = useState<Result<AssemblersJson>>();
  const [recpNames, setRecpNames] = useState<Result<Locale>>();
  const [itemNames, setItemNames] = useState<Result<Locale>>();
  const [fluidNames, setFluidNames] = useState<Result<Locale>>();
  const [entityNames, setEntityNames] = useState<Result<Locale>>();

  useEffect(() => fetchJson('/script-output/assemblers.json', setAssems), []);
  useEffect(
    () => fetchJson('/script-output/recipe-locale.json', setRecpNames),
    [],
  );
  useEffect(
    () => fetchJson('/script-output/item-locale.json', setItemNames),
    [],
  );
  useEffect(
    () => fetchJson('/script-output/fluid-locale.json', setFluidNames),
    [],
  );
  useEffect(
    () => fetchJson('/script-output/entity-locale.json', setEntityNames),
    [],
  );

  const wanted = [assems, recpNames, itemNames, fluidNames, entityNames];
  if (!wanted.every((v) => !!v)) {
    return <p>loading {wanted.map((v) => (v ? '?' : '✓'))}...</p>;
  }

  const firstError = wanted.find((v) => v?.error);

  if (firstError?.error) {
    return <p>load error: {JSON.stringify(serializeError(firstError))}</p>;
  }

  return (
    <AtlasContext.Provider
      value={{
        assemblers: assems!.value!.t,
        tick: assems!.value!.tick,
        recps: assems!.value!.recps,
        availableImages: {
          // TODO: nauvis
          nauvis: keysOf(assems!.value!.xys).map((k) => toPair(k)),
        },
        recpName: recpNames!.value!.names,
        entityName: entityNames!.value!.names,
        itemName: (v: IngredientPrototype | ProductPrototype) => {
          if (v.type === 'fluid') {
            const cand = fluidNames!.value!.names[v.name];
            if (cand) return cand;
          }
          return itemNames!.value!.names[v.name] || v.name;
        },
      }}
    >
      {children}
    </AtlasContext.Provider>
  );
}

interface AssemblersJson {
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
      ingredients: IngredientPrototype[] | {};
      products: ProductPrototype[] | {};
    }
  >;
  xys: Record<`${number}_${number}`, never>;
}

interface Locale {
  names: Record<string, string>;
}

function toPair(v: `${number}_${number}`): [number, number] {
  const [a, b, ...o] = v.split('_');
  if (o.length) throw new Error(`invalid pair: ${v}`);
  return [Number(a), Number(b)];
}
