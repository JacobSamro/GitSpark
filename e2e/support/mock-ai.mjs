import http from "node:http";

export async function startMockAiServer() {
  const requests = [];
  const server = http.createServer(async (request, response) => {
    let raw = "";
    for await (const chunk of request) {
      raw += chunk;
    }

    requests.push({
      headers: request.headers,
      body: JSON.parse(raw),
    });

    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(
      JSON.stringify({
        choices: [
          {
            message: {
              content: JSON.stringify({
                subject: "test: mocked ai summary",
                body: "Mocked body from local e2e server.",
              }),
            },
          },
        ],
      }),
    );
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return {
    requests,
    url: `http://127.0.0.1:${port}/v1/chat/completions`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}
