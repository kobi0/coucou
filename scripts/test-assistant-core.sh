#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/coucou-assistant-core.XXXXXX")"
trap 'rm -rf "$TEST_DIR"' EXIT
CORE=(
    NotchBuddy/Sources/CoucouKit/Assistant/AssistantTool.swift
    NotchBuddy/Sources/CoucouKit/Assistant/ActionPolicy.swift
    NotchBuddy/Sources/CoucouKit/Assistant/PendingActionStore.swift
    NotchBuddy/Sources/CoucouKit/Assistant/ActivityLog.swift
    NotchBuddy/Sources/CoucouKit/Assistant/ReminderRequest.swift
    NotchBuddy/Sources/CoucouKit/Assistant/MailDraftRequest.swift
    NotchBuddy/Sources/CoucouKit/Assistant/ToolCatalog.swift
    NotchBuddy/Sources/CoucouKit/Assistant/ToolSchemas.swift
    NotchBuddy/Sources/CoucouKit/Assistant/ToolCallExtractor.swift
    NotchBuddy/Sources/CoucouKit/Assistant/MailtoLink.swift
    NotchBuddy/Sources/CoucouKit/Assistant/AssistantSession.swift
    NotchBuddy/Sources/CoucouKit/Assistant/Voice.swift
    NotchBuddy/Sources/CoucouKit/Assistant/VoiceSession.swift
    NotchBuddy/Sources/CoucouKit/Assistant/SpeakableText.swift
    NotchBuddy/Sources/CoucouKit/Assistant/VoiceIO.swift
)
swiftc "${CORE[@]}" tests/AssistantCoreTests.swift -o "$TEST_DIR/assistant-core-tests"
"$TEST_DIR/assistant-core-tests"
swiftc "${CORE[@]}" tests/AssistantSessionTests.swift -o "$TEST_DIR/assistant-session-tests"
"$TEST_DIR/assistant-session-tests"
swiftc "${CORE[@]}" tests/VoiceTests.swift -o "$TEST_DIR/voice-tests"
"$TEST_DIR/voice-tests"
