import { api } from '../lib/fetch.ts';
import { UrlState } from '../index.tsx';
import { useState } from 'preact/hooks';
import { Setter } from '../lib/ts.ts';
import { ComponentChildren } from 'preact';

const ZR = 1.2;
export type Coord = [number, number];

export function SurfaceMap({
  xys,
  us,
  setUs,
  setMG,
  children,
}: {
  xys: [number, number][];
  us: UrlState;
  setUs: Setter<UrlState>;
  setMG: Setter<Coord>;
  children: ComponentChildren;
}) {
  const [dragStart, setDragStart] = useState<undefined | [Coord, Coord]>(
    undefined,
  );

  const { centre, viewWidth: vw } = us;

  // tile-size from the mod? I've already forgotten how all these numbers line up. image width / zoom?
  const H = 256;
  const viewBox = () => {
    const [x, y] = centre;
    return `${x - vw / 2} ${y - vw / 2} ${vw} ${vw}`;
  };

  function cursorToGame(ev: MouseEventWithTarget): Coord {
    const [px, py] = cursorToP(ev);
    const [cx, cy] = centre;
    return [px * vw + cx, py * vw + cy];
  }

  return (
    <svg
      viewBox={viewBox()}
      onWheel={(ev) => {
        const cf = 1 - 1 / ZR;
        const [cx, cy] = centre;
        const [ncx, ncy] = cursorToGame(ev);
        if (ev.deltaY < 0) {
          setUs((us) => ({
            ...us,
            viewWidth: vw / ZR,
            centre: [cx + cf * (ncx - cx), cy + cf * (ncy - cy)],
          }));
        } else if (ev.deltaY > 0) {
          // not right but closer
          setUs((us) => ({
            ...us,
            centre: [cx - cf * (ncx - cx), cy - cf * (ncy - cy)],
            viewWidth: vw * ZR,
          }));
        }

        ev.preventDefault();
      }}
      onMouseDown={(ev) => {
        // if (ev.shiftKey || ev.altKey) {
        setDragStart([centre, cursorToP(ev)]);
        ev.preventDefault();
        // }
      }}
      onMouseUp={() => setDragStart(undefined)}
      onMouseLeave={() => setDragStart(undefined)}
      onMouseMove={(ev) => {
        setMG(cursorToGame(ev));
        if (dragStart) {
          const [[ox, oy], [sx, sy]] = dragStart;
          const [nx, ny] = cursorToP(ev);
          const [dx, dy] = [(sx - nx) * vw, (sy - ny) * vw];
          setUs((us) => ({
            ...us,
            centre: [ox + dx, oy + dy],
          }));
        }
      }}
    >
      {xys.map(([tx, ty]) => (
        <image
          href={api(`/script-output/assemblers-${us.surface}-${tx}_${ty}.png`)}
          x={tx * H}
          y={ty * H}
          width={H}
        />
      ))}
      {children}
    </svg>
  );
}

type MouseEventWithTarget = Omit<MouseEvent, 'currentTarget'> & {
  readonly currentTarget: Element;
};

function cursorToP(ev: MouseEventWithTarget): Coord {
  const box = ev.currentTarget.getBoundingClientRect();
  let px = (ev.clientX - box.left) / box.width - 0.5;
  let py = (ev.clientY - box.top) / box.height - 0.5;

  // aspect ratio correction
  if (box.width < box.height) {
    py *= box.height / box.width;
  } else {
    px *= box.width / box.height;
  }
  return [px, py];
}
