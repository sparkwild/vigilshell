#import <Foundation/Foundation.h>
#import <UserNotifications/UserNotifications.h>
#include <node_api.h>
#include <algorithm>
#include <memory>
#include <mutex>
#include <vector>

// 只在 Electron Main 内运行，授权主体保持为宿主 .app；不修改通知 delegate。
struct Pending {
  std::mutex mutex;
  napi_threadsafe_function callback = nullptr;
};
struct Environment { std::vector<std::weak_ptr<Pending>> pending; };

static void Finish(const std::shared_ptr<Pending>& pending, const char* status) {
  std::lock_guard<std::mutex> guard(pending->mutex);
  if (!pending->callback) return;
  // 状态均为静态字符串，环境退出后队列清理也不依赖 ObjC 对象生命周期。
  napi_call_threadsafe_function(pending->callback, const_cast<char*>(status), napi_tsfn_nonblocking);
  napi_release_threadsafe_function(pending->callback, napi_tsfn_release);
  pending->callback = nullptr;
}
static void ReadStatus(std::shared_ptr<Pending> pending) {
  @try {
    [[UNUserNotificationCenter currentNotificationCenter] getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings* settings) {
      switch (settings.authorizationStatus) {
        case UNAuthorizationStatusNotDetermined: Finish(pending, "notDetermined"); break;
        case UNAuthorizationStatusDenied: Finish(pending, "denied"); break;
        case UNAuthorizationStatusAuthorized: Finish(pending, "authorized"); break;
        case UNAuthorizationStatusProvisional: Finish(pending, "provisional"); break;
        default: Finish(pending, "unknown"); break;
      }
    }];
  } @catch (NSException*) { Finish(pending, "unknown"); }
}
static void Deliver(napi_env env, napi_value callback, void*, void* data) {
  if (!env || !callback) return;
  napi_value status, receiver, result;
  if (napi_create_string_utf8(env, static_cast<const char*>(data), NAPI_AUTO_LENGTH, &status) != napi_ok) return;
  if (napi_get_undefined(env, &receiver) != napi_ok) return;
  napi_call_function(env, receiver, callback, 1, &status, &result);
}
static napi_value Start(napi_env env, napi_callback_info info, bool request) {
  size_t argc = 1;
  napi_value callback, name, result;
  napi_valuetype type;
  void* context = nullptr;
  if (napi_get_cb_info(env, info, &argc, &callback, nullptr, &context) != napi_ok || argc != 1 ||
      napi_typeof(env, callback, &type) != napi_ok || type != napi_function) {
    napi_throw_type_error(env, nullptr, "Expected an authorization callback");
    return nullptr;
  }
  auto* state = static_cast<Environment*>(context);
  auto pending = std::make_shared<Pending>();
  napi_create_string_utf8(env, "NotificationAuthorization", NAPI_AUTO_LENGTH, &name);
  if (napi_create_threadsafe_function(env, callback, nullptr, name, 1, 1, nullptr, nullptr,
                                    nullptr, Deliver, &pending->callback) != napi_ok) {
    napi_throw_error(env, nullptr, "Cannot start authorization operation");
    return nullptr;
  }
  napi_unref_threadsafe_function(env, pending->callback);
  std::erase_if(state->pending, [](const auto& item) { return item.expired(); });
  state->pending.emplace_back(pending);
  if (request) {
    @try {
      [[UNUserNotificationCenter currentNotificationCenter]
        requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
        completionHandler:^(BOOL, NSError* error) {
          if (error) Finish(pending, "unknown"); else ReadStatus(pending);
        }];
    } @catch (NSException*) { Finish(pending, "unknown"); }
  } else ReadStatus(pending);
  napi_get_undefined(env, &result);
  return result;
}
static napi_value Query(napi_env env, napi_callback_info info) { return Start(env, info, false); }
static napi_value Request(napi_env env, napi_callback_info info) { return Start(env, info, true); }
static void Cleanup(void* context) {
  auto* state = static_cast<Environment*>(context);
  for (const auto& weak : state->pending) {
    if (auto pending = weak.lock()) {
      std::lock_guard<std::mutex> guard(pending->mutex);
      if (pending->callback) {
        napi_release_threadsafe_function(pending->callback, napi_tsfn_abort);
        pending->callback = nullptr;
      }
    }
  }
  delete state;
}
static napi_value Init(napi_env env, napi_value exports) {
  auto* state = new Environment();
  napi_add_env_cleanup_hook(env, Cleanup, state);
  napi_property_descriptor methods[] = {
    {"query", nullptr, Query, nullptr, nullptr, nullptr, napi_default, state},
    {"request", nullptr, Request, nullptr, nullptr, nullptr, napi_default, state},
  };
  napi_define_properties(env, exports, 2, methods);
  return exports;
}
NAPI_MODULE(NODE_GYP_MODULE_NAME, Init)
