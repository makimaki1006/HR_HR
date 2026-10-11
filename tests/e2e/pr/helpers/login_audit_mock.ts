import { createServer } from 'node:http';

/** ログイン保持の E2E 専用。Rust プロセスを入れ替えても残る偽の監査サービス。 */
export async function startLoginAuditMock() {
  const accounts = new Map<string, { id: string; email: string; role: string }>();
  const sessions = new Map<string, { accountId: string; method: string; ended: boolean }>();
  const server = createServer(async (request, response) => {
    const chunks: Buffer[] = [];
    for await (const chunk of request) chunks.push(Buffer.from(chunk));
    const statement = JSON.parse(Buffer.concat(chunks).toString()).requests[0].stmt;
    const sql: string = statement.sql;
    const args: string[] = (statement.args ?? []).map((arg: { value?: string }) => arg.value ?? '');
    let names: string[] = [];
    let rows: string[][] = [];
    if (sql.startsWith('SELECT 1')) {
      names = ['1']; rows = [['1']];
    } else if (sql.startsWith('SELECT id, role FROM accounts')) {
      names = ['id', 'role'];
      const account = [...accounts.values()].find((account) => account.email === args[0]);
      if (account) rows = [[account.id, account.role]];
    } else if (sql.startsWith('INSERT INTO accounts')) {
      accounts.set(args[0], { id: args[0], email: args[1], role: args[2] });
    } else if (sql.startsWith('INSERT INTO login_sessions') && !sql.includes('failure_reason')) {
      sessions.set(args[0], { accountId: args[1], method: args[5], ended: false });
    } else if (sql.startsWith('UPDATE login_sessions SET ended_at')) {
      const session = sessions.get(args[1]);
      if (session) session.ended = true;
    } else if (sql.startsWith('SELECT s.account_id FROM login_sessions')) {
      names = ['account_id'];
      const session = sessions.get(args[0]);
      const account = session && accounts.get(session.accountId);
      if (session && !session.ended && account?.email.toLowerCase() === args[1].toLowerCase() && session.method === args[2]) {
        rows = [[session.accountId]];
      }
    } else if (sql.startsWith('SELECT disabled_at FROM accounts')) {
      names = ['disabled_at'];
    }
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ results: [
      { type: 'ok', response: { type: 'execute', result: {
        cols: names.map((name) => ({ name })),
        rows: rows.map((row) => row.map((value) => ({ type: 'text', value }))), affected_row_count: 1,
      } } },
      { type: 'ok', response: { type: 'close' } },
    ] }));
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const port = (server.address() as { port: number }).port;
  return {
    url: `http://127.0.0.1:${port}`,
    close: () => new Promise<void>((resolve) => { server.close(() => resolve()); server.closeAllConnections(); }),
  };
}
