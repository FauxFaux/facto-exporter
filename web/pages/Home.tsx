import {
  IngredientPrototype,
  ProductPrototype,
} from 'factorio-raw-types/prototypes';
import { useContext, useState } from 'preact/hooks';
import { minBy, Setter } from '../lib/ts.ts';
import { type UrlState } from '../index.tsx';
import { Coord, SurfaceMap } from './SurfaceMap.tsx';
import { AtlasContext } from './LoadAtlas.tsx';

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
  const [mg, setMG] = useState([0, 0] as Coord);
  const { assemblers, recps, availableImages, recpName } =
    useContext(AtlasContext);

  const surfaces = [
    ...new Set(Object.values(assemblers).map((a) => a.surface)),
  ].sort();

  const nearest = minBy(Object.entries(assemblers), ([, a]) => {
    const [mx, my] = mg;
    const [ax, ay] = a.position;
    return Math.pow(mx - ax, 2) + Math.pow(my - ay, 2);
  });

  const sameRecipe = Object.values(assemblers)
    .filter((a) => a.recipe === nearest?.[1]?.recipe)
    .map((a) => a.position);

  const needsItem = ea(recps[nearest?.[1]?.recipe ?? '']?.ingredients).map(
    (i) => i.name,
  );

  const producingRecipes = Object.entries(recps)
    .filter(([, v]) =>
      ea(v.products)
        .map((p) => p.name)
        .some((p) => needsItem?.includes(p)),
    )
    .map(([name]) => name);

  const producers = Object.values(assemblers)
    .filter((a) => producingRecipes.includes(a.recipe ?? ''))
    .map((a) => a.position);

  return (
    <div id={'with-map'}>
      <div>
        <SurfaceMap
          xys={availableImages[us.surface] ?? []}
          setUs={setUs}
          setMG={setMG}
          us={us}
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
        <form class={'button-group'}>
          {surfaces.map(
            /* TODO: onClick */ (id) => (
              <>
                <input
                  name={'surface'}
                  type={'radio'}
                  id={'home-sp-' + id}
                  checked={id === us.surface}
                />
                <label for={'home-sp-' + id}>{id}</label>
              </>
            ),
          )}
        </form>
        <p>
          {Object.values(assemblers).length} assemblers,{' '}
          {Object.values(recps).length} recipies
        </p>
        <p>{JSON.stringify(us)}</p>
      </div>
      <div>
        <p>{JSON.stringify(nearest)}</p>
        <ul>
          <li>id: {nearest?.[0]}</li>
          <li>recipe: {recpName[nearest?.[1]?.recipe ?? '']}</li>
        </ul>
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
