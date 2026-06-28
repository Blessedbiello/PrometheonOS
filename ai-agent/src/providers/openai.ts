/** OpenAI (GPT) provider — also drives any OpenAI-compatible endpoint via `baseURL`. */
import OpenAI from "openai";
import { buildPrompt } from "../prompts.js";
import { parseLlmDecision } from "../parse.js";
import type { DecisionRequest, LlmDecision, LlmProvider } from "./types.js";

export class OpenAiProvider implements LlmProvider {
  readonly name = "openai";
  private client: OpenAI;
  private model: string;

  /**
   * `baseURL` (optional) points at an OpenAI-compatible server instead of api.openai.com — e.g. a
   * Nosana inference endpoint, vLLM/TGI, Together, or Groq. `undefined` → the default OpenAI host.
   */
  constructor(apiKey: string, model = "gpt-4.1", baseURL?: string) {
    this.client = new OpenAI({ apiKey, baseURL });
    this.model = model;
  }

  async decide(req: DecisionRequest): Promise<LlmDecision> {
    const { system, user } = buildPrompt(req);
    const resp = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: "system", content: system },
        { role: "user", content: user },
      ],
      response_format: { type: "json_object" },
    });
    const text = resp.choices[0]?.message?.content ?? "";
    return parseLlmDecision(text);
  }
}
