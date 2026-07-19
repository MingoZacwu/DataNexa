#import <ServiceManagement/ServiceManagement.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Cocoa/Cocoa.h>
#import <libproc.h>
#import <string.h>
#import <unistd.h>

static pid_t parentProcess(pid_t pid) {
    struct proc_bsdinfo info = {0};
    int size = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info));
    return size == (int)sizeof(info) ? info.pbi_ppid : -1;
}

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
static pid_t realParentProcess(pid_t pid) {
    ProcessSerialNumber processSerialNumber = {0, kNoProcess};
    if (GetProcessForPID(pid, &processSerialNumber) != noErr) return -1;

    NSDictionary *processInfo = CFBridgingRelease(
        ProcessInformationCopyDictionary(&processSerialNumber,
                                          (UInt32)kProcessDictionaryIncludeAllInformationMask));
    NSNumber *parentSerialNumber = processInfo[@"ParentPSN"];
    if (![parentSerialNumber isKindOfClass:[NSNumber class]]) return -1;

    ProcessSerialNumber parentProcessSerialNumber = {0, kNoProcess};
    parentProcessSerialNumber.lowLongOfPSN =
        (UInt32)(parentSerialNumber.longLongValue & 0xFFFFFFFFLL);
    parentProcessSerialNumber.highLongOfPSN =
        (UInt32)((parentSerialNumber.longLongValue >> 32) & 0xFFFFFFFFLL);
    NSDictionary *parentInfo = CFBridgingRelease(
        ProcessInformationCopyDictionary(&parentProcessSerialNumber,
                                          (UInt32)kProcessDictionaryIncludeAllInformationMask));
    return [parentInfo[@"pid"] intValue];
}
#pragma clang diagnostic pop

static BOOL hasBundleIdentifier(NSString *identifier, pid_t pid) {
    NSRunningApplication *application = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
    return [application.bundleIdentifier isEqualToString:identifier];
}

static int copyServiceError(NSError *error, char *buffer, size_t bufferLength) {
    NSString *details = [NSString stringWithFormat:@"%@ (%@, code %ld)",
                         error.localizedDescription, error.domain, (long)error.code];
    if (buffer != NULL && bufferLength > 0) {
        strlcpy(buffer, details.UTF8String ?: "Unknown ServiceManagement error", bufferLength);
    }
    return (int)(error.code ?: -1);
}

int datanexa_register_login_item(char *errorBuffer, size_t errorBufferLength) {
    SMAppService *service = [SMAppService mainAppService];
    if (service.status == SMAppServiceStatusEnabled ||
        service.status == SMAppServiceStatusRequiresApproval) return 0;
    NSError *error = nil;
    return [service registerAndReturnError:&error]
        ? 0
        : copyServiceError(error, errorBuffer, errorBufferLength);
}

int datanexa_unregister_login_item(char *errorBuffer, size_t errorBufferLength) {
    SMAppService *service = [SMAppService mainAppService];
    if (service.status == SMAppServiceStatusNotRegistered || service.status == SMAppServiceStatusNotFound) return 0;
    NSError *error = nil;
    return [service unregisterAndReturnError:&error]
        ? 0
        : copyServiceError(error, errorBuffer, errorBufferLength);
}

int datanexa_login_item_status(void) {
    return (int)[SMAppService mainAppService].status;
}

int datanexa_launched_at_login(void) {
    pid_t realParent = realParentProcess(getpid());
    pid_t parent = realParent > 1 ? realParent : parentProcess(getpid());
    return parent > 1 && hasBundleIdentifier(@"com.apple.loginwindow", parent) ? 1 : 0;
}

int datanexa_set_activation_policy(int regular) {
    NSApplicationActivationPolicy policy = regular
        ? NSApplicationActivationPolicyRegular
        : NSApplicationActivationPolicyAccessory;
    return [NSApp setActivationPolicy:policy] ? 0 : -1;
}
