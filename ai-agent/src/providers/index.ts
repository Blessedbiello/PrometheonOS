/** Provider factory: select and construct the LLM provider from environment config. */
import { AnthropicProvider } from "./anthropic.js";
import { OpenAiProvider } from "./openai.js";
import { OllamaProvider } from "./ollama.js";
import { MockProvider } from "./mock.js";
import type { LlmProvider } from "./types.js";

export { AnthropicProvider, OpenAiProvider, OllamaProvider, MockProvider };
export type { LlmProvider } from "./types.js";

type Env = Record<string, string | undefined>;

/**
 * Build the configured provider. Selected by `LLM_PROVIDER` (default `anthropic`). Throws if the
 * selected provider's required credentials are missing, or the name is unknown.
 */
export function providerFromEnv(env: Env = process.env): LlmProvider {
  const which = (env.LLM_PROVIDER ?? "anthropic").toLowerCase();
  switch (which) {
    case "mock":
      return new MockProvider();
    case "anthropic": {
      const key = env.ANTHROPIC_API_KEY;
      if (!key) throw new Error("LLM_PROVIDER=anthropic requires ANTHROPIC_API_KEY");
      return new AnthropicProvider(key, env.ANTHROPIC_MODEL ?? "claude-opus-4-8");
    }
    case "openai": {
      const key = env.OPENAI_API_KEY;
      if (!key) throw new Error("LLM_PROVIDER=openai requires OPENAI_API_KEY");
      // OPENAI_BASE_URL targets any OpenAI-compatible endpoint (Nosana inference, vLLM, Together,
      // Groq, …); unset → api.openai.com.
      return new OpenAiProvider(key, env.OPENAI_MODEL ?? "gpt-4.1", env.OPENAI_BASE_URL);
    }
    case "ollama":
      return new OllamaProvider(env.OLLAMA_BASE_URL, env.OLLAMA_MODEL);
    default:
      throw new Error(`unknown LLM_PROVIDER: ${which}`);
  }
}
