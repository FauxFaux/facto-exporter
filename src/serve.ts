import * as fs from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import Fastify from 'fastify';
import { pino } from 'pino';
import fastifyStatic from '@fastify/static';
import fastifyCors from '@fastify/cors';

const logger = pino({
  level: 'warn',
  transport: {
    target: 'pino-pretty',
  },
});

export async function serve(args: string[]) {
  const [scriptOutputPath, unused] = args;
  if (!scriptOutputPath || unused) {
    console.error('Usage: serve path/to/script-output');
    return;
  }

  const root = path.resolve(scriptOutputPath);

  if (!existsSync(path.join(root, 'assemblers.json'))) {
    console.warn('Expecting existing `assemblers.json` in:', root);
    console.warn('Please run /write-screenshots in-game first.');
    return;
  }

  const fastify = Fastify({
    loggerInstance: logger,
  });

  fastify.get('/api/available/ticks', async (request, reply) => {
    return (await fs.readdir(root))
      .map((v) => /^production-(\d+)\.json$/.exec(v)?.[1])
      .filter((v) => v)
      .map((v) => Number(v))
      .sort((a, b) => a - b);
  });

  fastify.get('/script-output/:file', async (request, reply) => {
    const { file: rawFile } = request.params as { file: string };
    const requested = path.resolve(path.join(root, rawFile));
    if (!requested.startsWith(root)) {
      return reply.status(403).send('Forbidden');
    }
    switch (path.extname(requested)) {
      case '.json':
        reply.header('Content-Type', 'application/json');
        reply.send(await fs.readFile(requested, 'utf-8'));
        break;
      case '.png':
        reply.header('Content-Type', 'image/png');
        reply.send(await fs.readFile(requested));
        break;
      default:
        return reply.status(415).send('Unsupported Media Type');
    }
  });

  fastify.register(fastifyStatic, {
    root: path.join(import.meta.dirname, '..', 'dist'),
  });

  fastify.register(fastifyCors, {
    // vite dev server
    origin: 'http://localhost:5173',
    methods: ['GET', 'HEAD'],
    allowedHeaders: ['Content-Type'],
  });

  fastify.listen({ port: 3113 }, (err, address) => {
    if (err) throw err;
    console.log(`Server listening at ${address}`);
  });
}
