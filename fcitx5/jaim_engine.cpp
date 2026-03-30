/// Fcitx5 addon for JaIM — thin C++ wrapper over the Rust engine.

#include "jaim_engine.h"

#include <fcitx-utils/utf8.h>
#include <fcitx/candidatelist.h>
#include <fcitx/inputpanel.h>

namespace jaim {

// ── JaimState (per-InputContext) ─────────────────────────────────────────

JaimState::JaimState(JaimEngine *engine, fcitx::InputContext *ic)
    : engine_(engine), ic_(ic), ctx_(jaim_context_new()) {}

JaimState::~JaimState() {
    if (ctx_) {
        jaim_context_free(ctx_);
    }
}

void JaimState::keyEvent(fcitx::KeyEvent &event) {
    if (!ctx_)
        return;

    uint32_t sym = event.rawKey().sym();
    uint32_t state = event.rawKey().states();
    if (event.isRelease())
        state |= (1u << 30); // RELEASE_MASK

    if (jaim_handle_key(ctx_, sym, state)) {
        event.filterAndAccept();
    }
    // Always update UI after key events to keep preedit display in sync
    updateUI();
}

void JaimState::reset() {
    if (ctx_)
        jaim_reset(ctx_);
    ic_->inputPanel().reset();
    ic_->updatePreedit();
    ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

void JaimState::updateUI() {
    auto &panel = ic_->inputPanel();
    panel.reset();

    // Single FFI call to get all UI state
    JaimUiState ui{};
    jaim_get_ui_state(ctx_, &ui);

    // 1) Check for committed text
    if (ui.committed && ui.committed[0]) {
        ic_->commitString(ui.committed);
    }

    // 2) Update preedit
    if (ui.converting && ui.preedit && ui.preedit[0]) {
        // Conversion mode: show composed text with segment highlighting
        fcitx::Text preedit;
        std::string full(ui.preedit);

        auto charToBytes = [&full](int charPos) -> size_t {
            if (charPos <= 0) return 0;
            return fcitx::utf8::ncharByteLength(
                full.begin(), static_cast<size_t>(charPos));
        };

        for (int i = 0; i < ui.segment_count; i++) {
            int startCh = ui.segments[i].start_chars;
            int lenCh = ui.segments[i].char_len;
            size_t startByte = charToBytes(startCh);
            size_t endByte = charToBytes(startCh + lenCh);
            std::string segText = full.substr(startByte, endByte - startByte);

            auto flag = (i == ui.focus_index)
                ? fcitx::TextFormatFlag::HighLight
                : fcitx::TextFormatFlag::Underline;
            preedit.append(segText, flag);
        }
        preedit.setCursor(full.size());
        panel.setClientPreedit(preedit);

        // Build candidate list for focused segment
        if (ui.candidate_count > 0) {
            auto candList = std::make_unique<fcitx::CommonCandidateList>();
            candList->setPageSize(10);
            for (int j = 0; j < ui.candidate_count; j++) {
                if (ui.candidates[j]) {
                    candList->append<fcitx::DisplayOnlyCandidateWord>(
                            fcitx::Text(ui.candidates[j]));
                }
            }
            if (ui.selected_index >= 0 && ui.selected_index < ui.candidate_count) {
                candList->setGlobalCursorIndex(ui.selected_index);
            }
            panel.setCandidateList(std::move(candList));
        }
    } else if (ui.has_preedit && ui.preedit && ui.preedit[0]) {
        // Preedit mode: show raw kana
        fcitx::Text preedit;
        preedit.append(ui.preedit, fcitx::TextFormatFlag::Underline);
        preedit.setCursor(std::string(ui.preedit).size());
        panel.setClientPreedit(preedit);
    }

    ic_->updatePreedit();
    ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

// ── JaimEngine (addon) ──────────────────────────────────────────────────

JaimEngine::JaimEngine(fcitx::Instance *instance)
    : instance_(instance),
      factory_([this](fcitx::InputContext &ic) {
          return new JaimState(this, &ic);
      }) {
    instance_->inputContextManager().registerProperty("jaimState", &factory_);
}

std::vector<fcitx::InputMethodEntry> JaimEngine::listInputMethods() {
    std::vector<fcitx::InputMethodEntry> result;
    result.emplace_back("jaim", "JaIM - Japanese AI Input", "ja",
                        "jaim");
    return result;
}

void JaimEngine::keyEvent(const fcitx::InputMethodEntry & /*entry*/,
                          fcitx::KeyEvent &event) {
    auto *ic = event.inputContext();
    auto *state = ic->propertyFor(&factory_);
    state->keyEvent(event);
}

void JaimEngine::activate(const fcitx::InputMethodEntry & /*entry*/,
                          fcitx::InputContextEvent & /*event*/) {
    // Nothing special needed on activation
}

void JaimEngine::deactivate(const fcitx::InputMethodEntry & /*entry*/,
                            fcitx::InputContextEvent &event) {
    auto *ic = event.inputContext();
    auto *state = ic->propertyFor(&factory_);
    state->reset();
}

void JaimEngine::reset(const fcitx::InputMethodEntry & /*entry*/,
                       fcitx::InputContextEvent &event) {
    auto *ic = event.inputContext();
    auto *state = ic->propertyFor(&factory_);
    state->reset();
}

} // namespace jaim

FCITX_ADDON_FACTORY(jaim::JaimEngineFactory);
