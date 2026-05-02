/**
 * statusBar.ts — Status bar widget showing cumulative token savings.
 *
 * Polls `sqz stats --json` to read historical compression data from
 * ~/.sqz/sessions.db. Shows total tokens saved and average reduction
 * so the user sees real numbers, not a per-process budget counter
 * that resets on every engine restart.
 *
 * Requirement 6.4: display token savings in the editor status bar.
 */

import * as vscode from "vscode";
import { SqzBridge } from "./sqzBridge";

export interface SqzStats {
  totalCompressions: number;
  tokensIn: number;
  tokensOut: number;
  tokensSaved: number;
  avgReduction: number;
  cacheEntries: number;
  cacheSize: number;
}

export class SqzStatusBar {
  private item: vscode.StatusBarItem;
  private bridge: SqzBridge;
  private sessionId: string;
  private timer: ReturnType<typeof setInterval> | undefined;

  constructor(bridge: SqzBridge, sessionId: string) {
    this.bridge = bridge;
    this.sessionId = sessionId;

    this.item = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      100
    );
    this.item.command = "sqz.status";
    this.item.tooltip = "sqz token savings — click for details";
    this.item.text = "$(pulse) sqz";
    this.item.show();
  }

  /** Update the status bar with cumulative savings from sqz stats. */
  update(): void {
    try {
      const stats = this.bridge.getStats();
      if (stats.totalCompressions === 0) {
        this.item.text = "$(pulse) sqz: no data";
        this.item.tooltip = "sqz — no compressions recorded yet. Run some commands in Claude Code.";
        return;
      }
      const saved = stats.tokensSaved;
      const pct = Math.round(stats.avgReduction);
      const icon = saved > 1000 ? "$(check)" : "$(pulse)";
      this.item.text = `${icon} sqz: ${this.formatTokens(saved)} saved`;
      this.item.tooltip = [
        `sqz cumulative savings`,
        `Compressions: ${stats.totalCompressions.toLocaleString()}`,
        `Tokens in: ${stats.tokensIn.toLocaleString()}`,
        `Tokens out: ${stats.tokensOut.toLocaleString()}`,
        `Tokens saved: ${saved.toLocaleString()} (${pct}% avg)`,
        `Cache: ${stats.cacheEntries} entries`,
      ].join("\n");
    } catch {
      this.item.text = "sqz: --";
    }
  }

  /** Format token count for compact display. */
  private formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return `${n}`;
  }

  /** Notify the status bar that a compression happened (updates immediately). */
  onCompression(tokensOriginal: number, tokensCompressed: number): void {
    const saved = tokensOriginal - tokensCompressed;
    const pct = tokensOriginal > 0
      ? Math.round((saved / tokensOriginal) * 100)
      : 0;
    this.item.text = `$(check) sqz: saved ${pct}%`;
    // Refresh to real cumulative stats after a short delay
    setTimeout(() => this.update(), 1_500);
  }

  /** Start polling stats every `intervalMs` milliseconds. */
  startPolling(intervalMs = 30_000): void {
    this.stopPolling();
    this.update();
    this.timer = setInterval(() => this.update(), intervalMs);
  }

  /** Stop the polling timer. */
  stopPolling(): void {
    if (this.timer !== undefined) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
  }

  /** Update the session ID (e.g. when the user changes it in settings). */
  setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
    this.update();
  }

  dispose(): void {
    this.stopPolling();
    this.item.dispose();
  }
}
