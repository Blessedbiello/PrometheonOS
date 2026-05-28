/** OpenAI (GPT) provider. */
import OpenAI from "openai";
import { buildPrompt } from "../prompts.js";
import { parseLlmDecision } from "../parse.js";
import type { DecisionRequest, LlmDecision, LlmProvider } from "./types.js";

export class OpenAiProvider implements LlmProvider {
  readonly name = "openai";
  private client: OpenAI;
  private model: string;

  constructor(apiKey: string, model = "gpt-4.1") {
    this.client = new OpenAI({ apiKey });
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
