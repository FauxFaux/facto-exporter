import {
  IngredientPrototype,
  ProductPrototype,
} from 'factorio-raw-types/prototypes';
import { useEffect, useState } from 'preact/hooks';
import { fetchJson, Result } from '../lib/fetch.ts';
import { serializeError } from 'serialize-error';
import { keysOf, minBy, Setter } from '../lib/ts.ts';
import { type UrlState } from '../index.tsx';
import { Coord, SurfaceMap } from './SurfaceMap.tsx';

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
      ingredients: IngredientPrototype[] | {};
      products: ProductPrototype[] | {};
    }
  >;
  xys: Record<`${number}_${number}`, never>;
}

export function Home({ us, setUs }: { us: UrlState; setUs: Setter<UrlState> }) {
  const [ass, setAss] = useState<Result<Assemblers>>();
  const [mg, setMG] = useState([0, 0] as Coord);

  useEffect(() => fetchJson('/script-output/assemblers.json', setAss), []);

  if (!ass) {
    return <h1>initial data load</h1>;
  }

  if (ass.error) {
    return <h1>erroh: {JSON.stringify(serializeError(ass.error))}</h1>;
  }

  const nearest = minBy(Object.entries(ass.value.t), ([, a]) => {
    const [mx, my] = mg;
    const [ax, ay] = a.position;
    return Math.pow(mx - ax, 2) + Math.pow(my - ay, 2);
  });

  const sameRecipe = Object.values(ass.value.t)
    .filter((a) => a.recipe === nearest?.[1]?.recipe)
    .map((a) => a.position);

  const needsItem = ea(
    ass.value.recps[nearest?.[1]?.recipe ?? '']?.ingredients,
  ).map((i) => i.name);
  const producingRecipes = Object.entries(ass.value.recps)
    .filter(([, v]) =>
      ea(v.products)
        .map((p) => p.name)
        .some((p) => needsItem?.includes(p)),
    )
    .map(([name]) => name);

  const producers = Object.values(ass.value.t)
    .filter((a) => producingRecipes.includes(a.recipe ?? ''))
    .map((a) => a.position);

  // tile-size from the mod? I've already forgotten how all these numbers line up. image width / zoom?

  return (
    <div id={'with-map'}>
      <div>
        <SurfaceMap
          xys={keysOf(ass.value.xys).map((v) => toPair(v))}
          setUs={setUs}
          setMG={setMG}
          us={us}
          // TODO
          surface={'nauvis'}
        >
          {producers.map(([x, y]) => (
            <circle cx={x} cy={y} r={1} fill={'#008'} />
          ))}
          {sameRecipe.map(([x, y]) => (
            <circle cx={x} cy={y} r={1} fill={'#080'} />
          ))}
          {
            <circle
              cx={nearest?.[1].position[0]}
              cy={nearest?.[1].position[1]}
              r={1}
              fill={'#0f0'}
            />
          }
        </SurfaceMap>
      </div>
      <div style={'padding: 1em'}>
        <p>
          {Object.values(ass.value.t).length} assemblers,{' '}
          {Object.values(ass.value.recps).length} recipies
        </p>
        <p>{JSON.stringify(us)}</p>
      </div>
      <div>
        <p>{JSON.stringify(nearest)}</p>
      </div>
    </div>
  );
}

function ea<T>(v: T[] | Record<string, never> | undefined): T[] {
  if (Array.isArray(v)) {
    return v;
  }
  return [];
}

function toPair(v: `${number}_${number}`): [number, number] {
  const [a, b, ...o] = v.split('_');
  if (o.length) throw new Error(`invalid pair: ${v}`);
  return [Number(a), Number(b)];
}
