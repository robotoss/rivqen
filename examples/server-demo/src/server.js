// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Rivqen protocol (RQP) demo server: starts the server of src/app.js.
// Run: npm start   (PORT=8787 by default, binds to 127.0.0.1; ORIGIN sets the public origin)

import { createDemoServer } from './app.js';
import { log } from './log.js';

const PORT = Number(process.env.PORT ?? 8787);
const HOST = process.env.HOST ?? '127.0.0.1';
const ORIGIN = process.env.ORIGIN ?? `http://${HOST}:${PORT}`;

createDemoServer({ origin: ORIGIN }).listen(PORT, HOST, () => log('info', 'server.started', { origin: ORIGIN }));
