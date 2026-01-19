import { Atlas, AtlasContext } from './LoadAtlas.tsx';
import { useContext } from 'preact/hooks';
import { ea } from '../lib/ts.ts';
import { ProductionContext } from './WatchProduction.tsx';

export function AssemCard({
  assem,
}: {
  assem: [number, Atlas['assemblers'][number]];
}) {
  const [id, a] = assem;
  const { entityName } = useContext(AtlasContext);

  return (
    <div>
      <ul>
        <li>id: {id}</li>
        <li>
          recent production: <ProductionGraph id={id} />
        </li>
        <li>{entityName[a.name]}</li>
        <RecipeListing recipe={a.recipe} />
      </ul>
    </div>
  );
}

function RecipeListing({ recipe }: { recipe: string | undefined }) {
  const { recps, recpName, itemName } = useContext(AtlasContext);
  const r = recps[recipe ?? ''];
  if (!r) {
    return <li>unknown recipe {recipe}</li>;
  }
  return (
    <>
      <li>recipe: {recpName[recipe ?? '']}</li>
      <li>
        ingredients:
        <ul>
          {ea(r.ingredients).map((ing) => (
            <li>
              {itemName(ing)}: {ing.amount}
            </li>
          ))}
        </ul>
      </li>
      <li>
        products:
        <ul>
          {ea(r.products).map((prod) => (
            <li>
              {itemName(prod)}: {prod.amount}
            </li>
          ))}
        </ul>
      </li>
    </>
  );
}

function ProductionGraph({ id }: { id: number }) {
  const { byA } = useContext(ProductionContext);
  const raw = byA[id];
  if (!raw) {
    return <div>No production data</div>;
  }

  const deltas: number[] = [];

  for (let i = 1; i < raw.length; i++) {
    deltas.push(raw[i]! - raw[i - 1]!);
  }

  deltas.shift();

  const max = Math.max(...deltas);
  return (
    <svg width={400} height={200} viewBox={'0 0 400 200'}>
      <polyline
        fill="none"
        stroke="#007700"
        strokeWidth="2"
        points={deltas
          .map(
            (v, i) => `${(i / deltas.length) * 400},${200 - (v / max) * 200}`,
          )
          .join(' ')}
      />
      <text y={40} fill={'white'}>
        {max}
      </text>
    </svg>
  );
}
