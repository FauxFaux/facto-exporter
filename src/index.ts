import { parseArgs } from 'node:util';
import { serve } from './serve.ts';

const help = `
Usage: <subcommand>
Subcommands:
 serve path/to/script-output        Serve data from the specified script-output directory
`.trim();

export async function main() {
  const args = process.argv.slice(2);
  const { tokens } = parseArgs({
    options: {},
    tokens: true,
    strict: false,
  });

  const subTokenIdx = tokens.findIndex((e) => e.kind === 'positional');
  const subToken = subTokenIdx !== -1 ? tokens[subTokenIdx] : undefined;
  if (!subToken || subToken.kind !== 'positional') {
    console.log(help, '\n\nNo subcommand provided.');
    return;
  }

  const sub = subToken.value;
  const subArgs = args.slice(subToken.index + 1);

  switch (sub) {
    case 'serve': {
      await serve(subArgs);
      break;
    }
  }
}
