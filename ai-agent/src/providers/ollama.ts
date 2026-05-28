/** Local Ollama provider (no SDK; uses the HTTP chat API). */
import { buildPrompt } from "../prompts.js";
import { parseLlmDecision } from "../parse.js";
import type { DecisionRequest, LlmDecision, LlmProvider } from "./types.js";

export class OllamaProvider implements LlmProvider {
  readonly name = "ollama";
  private baseUrl: string;
  private model: string;

  constructor(baseUrl = "http://localhost:11434", model = "llama3.1") {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.model = model;
  }

  async decide(req: DecisionRequest): Promise<LlmDecision> {
    const { system, user } = buildPrompt(req);
    const resp = await fetch(`${this.baseUrl}/api/chat`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model: this.model,
        stream: false,
        format: "json", // constrain output to JSON
        messages: [
          { role: "system", content: system },
          { role: "user", content: user },
        ],
      }),
    });
    if (!resp.ok) {
      throw new Error(`ollama HTTP ${resp.status}`);
    }
    const data = (await resp.json()) as { message?: { content?: string } };
    return parseLlmDecision(data.message?.content ?? "");
  }
}
