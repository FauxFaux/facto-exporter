import { render } from 'preact';
import { LocationProvider, Router, Route } from 'preact-iso';

import { Home } from './pages/Home.tsx';
import './style.css';

export function App() {
  return (
    <LocationProvider>
      <main>
        <Router>
          <Route path="/" component={Home} />
          <Route default component={NotFound} />
        </Router>
      </main>
    </LocationProvider>
  );
}

function NotFound() {
  return (
    <section>
      <h1>404: Not Found</h1>
      <p>It's gone :(</p>
    </section>
  );
}

render(<App />, document.getElementById('app')!);
