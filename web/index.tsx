import { render } from 'preact';

import { Home } from './pages/Home.tsx';
import './style.css';
import { useEffect, useState } from 'preact/hooks';
import { debounce } from './lib/ts.ts';

export interface UrlState {
  centre: [number, number];
  viewWidth: number;
  surface: string;
}

const setHash = debounce((v: UrlState) => {
  window.location.hash = packUs(v);
}, 50);

export function App() {
  const [us, setUs] = useState<UrlState>(
    window.location.hash.length > 5
      ? unpackUs(window.location.hash)
      : {
          centre: [0, 0],
          viewWidth: 400,
          surface: 'nauvis',
        },
  );

  useEffect(() => {
    window.onhashchange = () => setUs(unpackUs(window.location.hash));
  }, []);

  useEffect(() => setHash(us), [us]);

  return <Home us={us} setUs={setUs} />;
}

function packUs(us: UrlState) {
  const ff = (d: number) => Math.round(d * 100) / 100;
  return `#${ff(us.centre[0])}#${ff(us.centre[1])}#${ff(us.viewWidth)}#${us.surface}`;
}

function unpackUs(hash: string): UrlState {
  const parts = hash.split('#');
  const [cx, cy, vw] = parts.slice(1, 4).map(Number);
  return {
    centre: [cx || 0, cy || 0],
    viewWidth: vw || 400,
    surface: parts[4] || 'nauvis',
  };
}

render(<App />, document.getElementById('app')!);
