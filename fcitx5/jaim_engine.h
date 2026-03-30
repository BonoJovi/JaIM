/// Fcitx5 addon for JaIM — thin C++ wrapper over the Rust engine.

#ifndef JAIM_ENGINE_H
#define JAIM_ENGINE_H

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/instance.h>

#include "jaim_ffi.h"

namespace jaim {

class JaimEngine;

/// Per-InputContext state wrapping a JaimContext (Rust engine).
class JaimState : public fcitx::InputContextProperty {
public:
    JaimState(JaimEngine *engine, fcitx::InputContext *ic);
    ~JaimState();

    void keyEvent(fcitx::KeyEvent &event);
    void reset();

private:
    void updateUI();

    JaimEngine *engine_;
    fcitx::InputContext *ic_;
    JaimContext *ctx_;
};

/// Fcitx5 input method engine addon.
class JaimEngine : public fcitx::InputMethodEngineV2 {
public:
    JaimEngine(fcitx::Instance *instance);

    void keyEvent(const fcitx::InputMethodEntry &entry,
                  fcitx::KeyEvent &event) override;
    void activate(const fcitx::InputMethodEntry &entry,
                  fcitx::InputContextEvent &event) override;
    void deactivate(const fcitx::InputMethodEntry &entry,
                    fcitx::InputContextEvent &event) override;
    void reset(const fcitx::InputMethodEntry &entry,
               fcitx::InputContextEvent &event) override;

    std::vector<fcitx::InputMethodEntry> listInputMethods() override;

    auto &factory() { return factory_; }

private:
    fcitx::Instance *instance_;
    fcitx::FactoryFor<JaimState> factory_;
};

class JaimEngineFactory : public fcitx::AddonFactory {
    fcitx::AddonInstance *
    create(fcitx::AddonManager *manager) override {
        return new JaimEngine(manager->instance());
    }
};

} // namespace jaim

#endif // JAIM_ENGINE_H
