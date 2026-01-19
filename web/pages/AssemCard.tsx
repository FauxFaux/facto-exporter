import { Atlas, AtlasContext } from './LoadAtlas.tsx';
import { useContext } from 'preact/hooks';
import { ea } from '../lib/ts.ts';

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
