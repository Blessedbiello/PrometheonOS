/** Anthropic (Claude) provider. */
import Anthropic from "@anthropic-ai/sdk";
import { buildPrompt } from "../prompts.js";
import { parseLlmDecision } from "../parse.js";
import type { DecisionRequest, LlmDecision, LlmProvider } from "./types.js";

export class AnthropicProvider implements LlmProvider {
  readonly name = "anthropic";
  private client: Anthropic;
  private model: string;

  constructor(apiKey: string, model = "claude-opus-4-7") {
    this.client = new Anthropic({ apiKey });
    this.model = model;
  }

  async decide(req: DecisionRequest): Promise<LlmDecision> {
    const { system, user } = buildPrompt(req);
    const resp = await this.client.messages.create({
      model: this.model,
      max_tokens: 1024,
      system,
      messages: [{ role: "user", content: user }],
    });
    const text = resp.content
      .map((block) => (block.type === "text" ? block.text : ""))
      .join("");
    return parseLlmDecision(text);
  }
}
