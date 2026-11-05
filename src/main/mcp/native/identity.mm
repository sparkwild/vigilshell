#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <LocalAuthentication/LocalAuthentication.h>
#include <node_api.h>
#include <memory>
#include <mutex>
#include <optional>

struct Completion { const char* outcome; int64_t code; };
struct Request {
  std::mutex mutex;
  napi_threadsafe_function callback = nullptr;
  LAContext *__strong context = nil;
  std::optional<Completion> completed;
};
struct State { std::shared_ptr<Request> pending; };
static napi_value Result(napi_env env, const Completion& completion) {
  napi_value result, outcome, code;
  napi_create_object(env, &result);
  napi_create_string_utf8(env, completion.outcome, NAPI_AUTO_LENGTH, &outcome);
  napi_create_int64(env, completion.code, &code);
  napi_set_named_property(env, result, "outcome", outcome);
  napi_set_named_property(env, result, "nativeCode", code);
  return result;
}
static void Queue(const std::shared_ptr<Request>& request, Completion completion) {
  auto* value = new Completion(completion);
  if (napi_call_threadsafe_function(request->callback, value, napi_tsfn_nonblocking) != napi_ok) delete value;
  napi_release_threadsafe_function(request->callback, napi_tsfn_release);
  request->callback = nullptr;
}
static void Finish(const std::shared_ptr<Request>& request, Completion completion) {
  std::lock_guard<std::mutex> guard(request->mutex);
  if (!request->callback) return;
  request->completed = completion;
  Queue(request, completion);
  request->context = nil;
}
static std::optional<Completion> CancelRequest(const std::shared_ptr<Request>& request, bool teardown = false) {
  LAContext* context;
  {
    std::lock_guard<std::mutex> guard(request->mutex);
    // Preserve a result already produced by LocalAuthentication but not yet delivered to JS.
    if (request->completed) return request->completed;
    context = request->context;
    request->context = nil;
    request->completed = Completion{"applicationCancelled", LAErrorAppCancel};
    if (request->callback) {
      if (!teardown) Queue(request, *request->completed);
      else { napi_release_threadsafe_function(request->callback, napi_tsfn_abort); request->callback = nullptr; }
    }
  }
  [context invalidate];
  return std::nullopt;
}
static void Deliver(napi_env env, napi_value callback, void*, void* value) {
  std::unique_ptr<Completion> completion(static_cast<Completion*>(value));
  if (!env || !callback || !completion) return;
  napi_value result = Result(env, *completion), receiver, returned;
  napi_get_undefined(env, &receiver);
  napi_call_function(env, receiver, callback, 1, &result, &returned);
}
static Completion Outcome(BOOL success, NSError* error) {
  if (success) return {"approved", 0};
  if (![error.domain isEqualToString:LAErrorDomain]) return {"nativeFailure", 0};
  switch (error.code) {
    case LAErrorUserCancel: return {"userCancelled", error.code};
    case LAErrorSystemCancel: return {"systemCancelled", error.code};
    case LAErrorAppCancel: return {"applicationCancelled", error.code};
    case LAErrorInvalidContext: return {"invalidContext", error.code};
    case LAErrorAuthenticationFailed: return {"authenticationFailed", error.code};
    case LAErrorPasscodeNotSet:
    case LAErrorBiometryNotAvailable:
    case LAErrorBiometryNotEnrolled:
    case LAErrorBiometryLockout:
    case LAErrorNotInteractive: return {"unavailable", error.code};
    default: return {"nativeFailure", error.code};
  }
}
static napi_value Verify(napi_env env, napi_callback_info info) {
  size_t argc = 2; napi_value args[2], result, resource;
  void* data = nullptr;
  napi_get_cb_info(env, info, &argc, args, nullptr, &data);
  auto* state = static_cast<State*>(data);
  napi_valuetype kind;
  if (argc != 2 || napi_typeof(env,args[1],&kind)!=napi_ok || kind!=napi_function) {
    napi_throw_type_error(env,nullptr,"Invalid authentication request"); return nullptr;
  }
  char reason[2048]; size_t length=0;
  if (napi_get_value_string_utf8(env,args[0],reason,sizeof(reason),&length)!=napi_ok || !length || length>=sizeof(reason)-1) {
    napi_throw_type_error(env,nullptr,"Invalid authentication reason"); return nullptr;
  }
  if (state->pending) CancelRequest(state->pending);
  auto request=std::make_shared<Request>(); state->pending=request;
  napi_create_string_utf8(env,"McpDeviceOwnerAuthentication",NAPI_AUTO_LENGTH,&resource);
  if (napi_create_threadsafe_function(env,args[1],nullptr,resource,1,1,nullptr,nullptr,nullptr,Deliver,&request->callback)!=napi_ok) {
    napi_throw_error(env,nullptr,"Authentication unavailable"); return nullptr;
  }
  napi_unref_threadsafe_function(env,request->callback);
  NSString* message=[[NSString alloc] initWithBytes:reason length:length encoding:NSUTF8StringEncoding];
  dispatch_async(dispatch_get_main_queue(), ^{
    @try {
      LAContext* context;
      {
        std::lock_guard<std::mutex> guard(request->mutex);
        if (!request->callback) return;
        context=[[LAContext alloc] init];
        request->context=context;
      }
      context.touchIDAuthenticationAllowableReuseDuration=0;
      NSError* error=nil;
      if (![context canEvaluatePolicy:LAPolicyDeviceOwnerAuthentication error:&error]) { Finish(request, Completion{"unavailable", error.code}); return; }
      [context evaluatePolicy:LAPolicyDeviceOwnerAuthentication localizedReason:message reply:^(BOOL success,NSError* error) { Finish(request,Outcome(success,error)); }];
    } @catch (NSException*) { Finish(request,Completion{"nativeFailure", 0}); }
  });
  napi_get_undefined(env,&result); return result;
}
static napi_value Cancel(napi_env env,napi_callback_info info) {
  void* data=nullptr; napi_get_cb_info(env,info,nullptr,nullptr,nullptr,&data);
  auto* state=static_cast<State*>(data);
  if (state->pending) {
    auto completed = CancelRequest(state->pending);
    if (completed) return Result(env, *completed);
  }
  napi_value result; napi_get_undefined(env,&result); return result;
}
static void Cleanup(void* data) {
  auto* state=static_cast<State*>(data);
  if (state->pending) CancelRequest(state->pending, true);
  delete state;
}
static napi_value FindApplication(napi_env env, napi_callback_info info) {
  size_t argc=2; napi_value args[2], result; char identifier[256]; size_t length=0;
  napi_get_cb_info(env,info,&argc,args,nullptr,nullptr);
  if(argc<1 || napi_get_value_string_utf8(env,args[0],identifier,sizeof(identifier),&length)!=napi_ok || !length || length>=sizeof(identifier)-1) {
    napi_throw_type_error(env,nullptr,"Invalid application identifier"); return nullptr;
  }
  @try {
    NSString* bundle=[[NSString alloc] initWithBytes:identifier length:length encoding:NSUTF8StringEncoding];
    NSURL* url=nil;
    napi_valuetype type=napi_undefined;
    if(argc==2) napi_typeof(env,args[1],&type);
    if(type!=napi_undefined) {
      char path[4096]; size_t size=0;
      if(type!=napi_string || napi_get_value_string_utf8(env,args[1],path,sizeof(path),&size)!=napi_ok || !size || size>=sizeof(path)-1) {
        napi_throw_type_error(env,nullptr,"Invalid application path"); return nullptr;
      }
      NSString* candidate=[[NSString alloc] initWithBytes:path length:size encoding:NSUTF8StringEncoding];
      if(!candidate.isAbsolutePath) { napi_throw_type_error(env,nullptr,"Invalid application path"); return nullptr; }
      url=[NSURL fileURLWithPath:candidate];
    } else {
      url=[[NSWorkspace sharedWorkspace] URLForApplicationWithBundleIdentifier:bundle];
    }
    NSBundle* application=url ? [NSBundle bundleWithURL:url] : nil;
    NSString* executable=application.executablePath;
    BOOL directory=NO;
    if([application.bundleIdentifier isEqualToString:bundle] && executable &&
       [[NSFileManager defaultManager] fileExistsAtPath:executable isDirectory:&directory] && !directory &&
       [[NSFileManager defaultManager] isExecutableFileAtPath:executable]) {
      napi_create_string_utf8(env,url.path.UTF8String,NAPI_AUTO_LENGTH,&result); return result;
    }
  } @catch(NSException*) { napi_throw_error(env,nullptr,"Application discovery unavailable"); return nullptr; }
  napi_get_undefined(env,&result); return result;
}
static napi_value Init(napi_env env,napi_value exports) {
  auto* state=new State(); napi_add_env_cleanup_hook(env,Cleanup,state);
  napi_property_descriptor methods[]={
    {"findApplication",nullptr,FindApplication,nullptr,nullptr,nullptr,napi_default,state},
    {"verify",nullptr,Verify,nullptr,nullptr,nullptr,napi_default,state},
    {"cancel",nullptr,Cancel,nullptr,nullptr,nullptr,napi_default,state}
  };
  napi_define_properties(env,exports,3,methods); return exports;
}
NAPI_MODULE(NODE_GYP_MODULE_NAME,Init)
