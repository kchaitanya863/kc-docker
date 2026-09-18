// boxr-vz: Native Apple Virtualization.framework Container Micro-VM Runner
//
// Replaces Docker daemon on macOS with a direct, ultra-fast, zero-overhead
// hypervisor micro-VM using Apple's native Virtualization.framework and virtio-fs.

#import <Foundation/Foundation.h>
#import <Virtualization/Virtualization.h>
#include <signal.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <poll.h>
#include <pthread.h>

static VZVirtualMachine *g_vm = nil;
static NSString *g_bundlePath = nil;

struct PortForwardSpec {
    int host_port;
    int vm_port;
    char host_ip[64];
    char vm_ip[64];
};

static void *proxy_stream_worker(void *arg) {
    int *fds = (int *)arg;
    int client_fd = fds[0];
    int target_fd = fds[1];
    free(fds);

    struct pollfd pfd[2];
    pfd[0].fd = client_fd;
    pfd[0].events = POLLIN;
    pfd[1].fd = target_fd;
    pfd[1].events = POLLIN;

    char buf[16384];
    while (1) {
        int ret = poll(pfd, 2, 300000); // 5 min timeout
        if (ret <= 0) break;

        if (pfd[0].revents & POLLIN) {
            ssize_t n = read(client_fd, buf, sizeof(buf));
            if (n <= 0) break;
            ssize_t written = 0;
            while (written < n) {
                ssize_t w = write(target_fd, buf + written, n - written);
                if (w <= 0) break;
                written += w;
            }
            if (written < n) break;
        }
        if (pfd[1].revents & POLLIN) {
            ssize_t n = read(target_fd, buf, sizeof(buf));
            if (n <= 0) break;
            ssize_t written = 0;
            while (written < n) {
                ssize_t w = write(client_fd, buf + written, n - written);
                if (w <= 0) break;
                written += w;
            }
            if (written < n) break;
        }
        if ((pfd[0].revents & (POLLERR | POLLHUP | POLLNVAL)) ||
            (pfd[1].revents & (POLLERR | POLLHUP | POLLNVAL))) {
            break;
        }
    }

    close(client_fd);
    close(target_fd);
    return NULL;
}

static void *port_forward_listener_thread(void *arg) {
    struct PortForwardSpec *spec = (struct PortForwardSpec *)arg;
    int server_fd = -1;

    for (int attempt = 0; attempt < 50; attempt++) {
        server_fd = socket(AF_INET, SOCK_STREAM, 0);
        if (server_fd < 0) break;

        int opt = 1;
        setsockopt(server_fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));
        setsockopt(server_fd, SOL_SOCKET, SO_REUSEPORT, &opt, sizeof(opt));

        struct sockaddr_in saddr;
        memset(&saddr, 0, sizeof(saddr));
        saddr.sin_family = AF_INET;
        saddr.sin_port = htons(spec->host_port);
        inet_pton(AF_INET, spec->host_ip, &saddr.sin_addr);

        if (bind(server_fd, (struct sockaddr *)&saddr, sizeof(saddr)) == 0) {
            break;
        }
        close(server_fd);
        server_fd = -1;
        usleep(100000); // 100ms
    }

    if (server_fd < 0) {
        free(spec);
        return NULL;
    }

    if (listen(server_fd, 128) < 0) {
        close(server_fd);
        free(spec);
        return NULL;
    }

    while (1) {
        struct sockaddr_in client_addr;
        socklen_t client_len = sizeof(client_addr);
        int client_fd = accept(server_fd, (struct sockaddr *)&client_addr, &client_len);
        if (client_fd < 0) break;

        // Connect to container target inside micro-VM (retry up to 3 seconds if container is booting)
        int target_fd = -1;
        for (int attempt = 0; attempt < 30; attempt++) {
            target_fd = socket(AF_INET, SOCK_STREAM, 0);
            if (target_fd < 0) break;

            struct sockaddr_in taddr;
            memset(&taddr, 0, sizeof(taddr));
            taddr.sin_family = AF_INET;
            taddr.sin_port = htons(spec->vm_port);
            inet_pton(AF_INET, spec->vm_ip, &taddr.sin_addr);

            if (connect(target_fd, (struct sockaddr *)&taddr, sizeof(taddr)) == 0) {
                break;
            }
            close(target_fd);
            target_fd = -1;
            usleep(100000); // 100ms
        }

        if (target_fd < 0) {
            close(client_fd);
            continue;
        }

        int *fds = malloc(sizeof(int) * 2);
        fds[0] = client_fd;
        fds[1] = target_fd;

        pthread_t t;
        pthread_attr_t attr;
        pthread_attr_init(&attr);
        pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
        pthread_create(&t, &attr, proxy_stream_worker, fds);
        pthread_attr_destroy(&attr);
    }

    close(server_fd);
    free(spec);
    return NULL;
}

static void clean_exit_handler(void) {
    if (g_vm && [g_vm canStop]) {
        dispatch_semaphore_t sem = dispatch_semaphore_create(0);
        [g_vm stopWithCompletionHandler:^(NSError * _Nullable error) {
            dispatch_semaphore_signal(sem);
        }];
        dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3.0 * NSEC_PER_SEC)));
    }
}

@interface BoxrVMDelegate : NSObject <VZVirtualMachineDelegate>
@property (nonatomic, assign) BOOL finished;
@property (nonatomic, assign) int exitCode;
@end

@implementation BoxrVMDelegate
- (void)guestDidStopVirtualMachine:(VZVirtualMachine *)virtualMachine {
    self.finished = YES;
    CFRunLoopStop(CFRunLoopGetMain());
}

- (void)virtualMachine:(VZVirtualMachine *)virtualMachine didStopWithError:(NSError *)error {
    self.finished = YES;
    CFRunLoopStop(CFRunLoopGetMain());
}
@end

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSString *bundlePath = nil;
        NSString *rootfsPath = nil;
        NSString *kernelPath = nil;
        NSString *initrdPath = nil;
        BOOL isDetach = NO;
        NSUInteger cpuCount = 2;
        unsigned long long memoryBytes = 512 * 1024 * 1024ULL;

        NSMutableArray<NSString *> *mountSpecs = [NSMutableArray array];
        NSMutableArray<NSString *> *portSpecs = [NSMutableArray array];

        for (int i = 1; i < argc; i++) {
            NSString *arg = [NSString stringWithUTF8String:argv[i]];
            if ([arg isEqualToString:@"--bundle"] && i + 1 < argc) {
                bundlePath = [NSString stringWithUTF8String:argv[++i]];
            } else if ([arg isEqualToString:@"--rootfs"] && i + 1 < argc) {
                rootfsPath = [NSString stringWithUTF8String:argv[++i]];
            } else if ([arg isEqualToString:@"--mount"] && i + 1 < argc) {
                [mountSpecs addObject:[NSString stringWithUTF8String:argv[++i]]];
            } else if ([arg isEqualToString:@"--port"] && i + 1 < argc) {
                [portSpecs addObject:[NSString stringWithUTF8String:argv[++i]]];
            } else if ([arg isEqualToString:@"--kernel"] && i + 1 < argc) {
                kernelPath = [NSString stringWithUTF8String:argv[++i]];
            } else if ([arg isEqualToString:@"--initrd"] && i + 1 < argc) {
                initrdPath = [NSString stringWithUTF8String:argv[++i]];
            } else if ([arg isEqualToString:@"--detach"]) {
                isDetach = YES;
            } else if ([arg isEqualToString:@"--cpus"] && i + 1 < argc) {
                cpuCount = (NSUInteger)atoi(argv[++i]);
            } else if ([arg isEqualToString:@"--memory"] && i + 1 < argc) {
                memoryBytes = (unsigned long long)atoll(argv[++i]);
            }
        }

        if (!rootfsPath) {
            fprintf(stderr, "Error: --rootfs is required for boxr-vz\n");
            return 1;
        }

        NSString *homeDir = NSHomeDirectory();
        if (!kernelPath) {
            kernelPath = [homeDir stringByAppendingPathComponent:@".boxr/vm/vmlinux"];
        }
        if (!initrdPath) {
            initrdPath = [homeDir stringByAppendingPathComponent:@".boxr/vm/initrd.cpio.gz"];
        }

        if (![[NSFileManager defaultManager] fileExistsAtPath:kernelPath]) {
            fprintf(stderr, "Error: Linux kernel not found at %s\n", [kernelPath UTF8String]);
            return 1;
        }

        if (![[NSFileManager defaultManager] fileExistsAtPath:initrdPath]) {
            fprintf(stderr, "Error: Micro-VM initrd not found at %s\n", [initrdPath UTF8String]);
            return 1;
        }

        atexit(clean_exit_handler);

        signal(SIGINT, SIG_IGN);
        signal(SIGTERM, SIG_IGN);

        void (^stopAndExit)(int) = ^(int sig) {
            if (g_vm && [g_vm canStop]) {
                dispatch_semaphore_t sem = dispatch_semaphore_create(0);
                [g_vm stopWithCompletionHandler:^(NSError * _Nullable errorOrNil) {
                    dispatch_semaphore_signal(sem);
                }];
                dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3.0 * NSEC_PER_SEC)));
            }
            if (bundlePath) {
                NSString *pidFile = [bundlePath stringByAppendingPathComponent:@"vm.pid"];
                [[NSFileManager defaultManager] removeItemAtPath:pidFile error:nil];
            }
            exit(128 + sig);
        };

        dispatch_source_t sigtermSrc = dispatch_source_create(DISPATCH_SOURCE_TYPE_SIGNAL, SIGTERM, 0, dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0));
        dispatch_source_set_event_handler(sigtermSrc, ^{
            stopAndExit(SIGTERM);
        });
        dispatch_resume(sigtermSrc);

        dispatch_source_t sigintSrc = dispatch_source_create(DISPATCH_SOURCE_TYPE_SIGNAL, SIGINT, 0, dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0));
        dispatch_source_set_event_handler(sigintSrc, ^{
            stopAndExit(SIGINT);
        });
        dispatch_resume(sigintSrc);

        // Record PID if bundle path is provided
        if (bundlePath) {
            g_bundlePath = bundlePath;
            NSString *pidFile = [bundlePath stringByAppendingPathComponent:@"vm.pid"];
            NSString *pidStr = [NSString stringWithFormat:@"%d\n", getpid()];
            [pidStr writeToFile:pidFile atomically:YES encoding:NSUTF8StringEncoding error:nil];
        }

        NSURL *kernelURL = [NSURL fileURLWithPath:kernelPath];
        NSURL *initrdURL = [NSURL fileURLWithPath:initrdPath];

        VZLinuxBootLoader *bootloader = [[VZLinuxBootLoader alloc] initWithKernelURL:kernelURL];
        bootloader.initialRamdiskURL = initrdURL;
        bootloader.commandLine = @"console=hvc0 quiet loglevel=3 random.trust_cpu=on random.trust_bootloader=on panic=1";

        VZVirtualMachineConfiguration *config = [[VZVirtualMachineConfiguration alloc] init];
        config.bootLoader = bootloader;
        config.CPUCount = cpuCount;
        config.memorySize = memoryBytes;

        NSMutableArray<VZVirtioFileSystemDeviceConfiguration *> *sharingDevices = [NSMutableArray array];

        // virtio-fs share for container rootfs
        VZSharedDirectory *dir = [[VZSharedDirectory alloc] initWithURL:[NSURL fileURLWithPath:rootfsPath] readOnly:NO];
        VZSingleDirectoryShare *share = [[VZSingleDirectoryShare alloc] initWithDirectory:dir];
        VZVirtioFileSystemDeviceConfiguration *fsConfig = [[VZVirtioFileSystemDeviceConfiguration alloc] initWithTag:@"boxr_rootfs"];
        fsConfig.share = share;
        [sharingDevices addObject:fsConfig];

        // Additional directory mounts
        for (NSString *mSpec in mountSpecs) {
            NSArray *parts = [mSpec componentsSeparatedByString:@"="];
            if ([parts count] == 2) {
                NSString *tag = parts[0];
                NSString *mPath = parts[1];
                if ([[NSFileManager defaultManager] fileExistsAtPath:mPath]) {
                    VZSharedDirectory *mDir = [[VZSharedDirectory alloc] initWithURL:[NSURL fileURLWithPath:mPath] readOnly:NO];
                    VZSingleDirectoryShare *mShare = [[VZSingleDirectoryShare alloc] initWithDirectory:mDir];
                    VZVirtioFileSystemDeviceConfiguration *mFsConfig = [[VZVirtioFileSystemDeviceConfiguration alloc] initWithTag:tag];
                    mFsConfig.share = mShare;
                    [sharingDevices addObject:mFsConfig];
                }
            }
        }
        config.directorySharingDevices = sharingDevices;

        // NAT networking for container outbound traffic
        VZVirtioNetworkDeviceConfiguration *netConfig = [[VZVirtioNetworkDeviceConfiguration alloc] init];
        netConfig.attachment = [[VZNATNetworkDeviceAttachment alloc] init];
        config.networkDevices = @[netConfig];

        // Virtio Entropy (RNG) device for instant /dev/urandom and getrandom entropy
        VZVirtioEntropyDeviceConfiguration *entropyConfig = [[VZVirtioEntropyDeviceConfiguration alloc] init];
        config.entropyDevices = @[entropyConfig];

        // Serial console
        VZVirtioConsoleDeviceSerialPortConfiguration *consoleConfig = [[VZVirtioConsoleDeviceSerialPortConfiguration alloc] init];
        NSPipe *dummyInPipe = [NSPipe pipe];
        NSFileHandle *readHandle = [NSFileHandle fileHandleWithStandardInput];
        NSFileHandle *writeHandle = [NSFileHandle fileHandleWithStandardOutput];

        if (isDetach && bundlePath) {
            NSString *logFile = [bundlePath stringByAppendingPathComponent:@"logs.txt"];
            [[NSFileManager defaultManager] createFileAtPath:logFile contents:nil attributes:nil];
            writeHandle = [NSFileHandle fileHandleForWritingAtPath:logFile];
            readHandle = [dummyInPipe fileHandleForReading];
        }

        VZFileHandleSerialPortAttachment *attachment = [[VZFileHandleSerialPortAttachment alloc] 
            initWithFileHandleForReading:readHandle 
            fileHandleForWriting:writeHandle];
        consoleConfig.attachment = attachment;
        config.serialPorts = @[consoleConfig];

        NSError *validateError = nil;
        if (![config validateWithError:&validateError]) {
            fprintf(stderr, "Error: Invalid VM configuration: %s\n", [[validateError localizedDescription] UTF8String]);
            return 1;
        }

        VZVirtualMachine *vm = [[VZVirtualMachine alloc] initWithConfiguration:config];
        g_vm = vm;

        BoxrVMDelegate *delegate = [[BoxrVMDelegate alloc] init];
        vm.delegate = delegate;

        dispatch_semaphore_t startSem = dispatch_semaphore_create(0);
        __block NSError *startError = nil;

        [vm startWithCompletionHandler:^(NSError * _Nullable errorIfFailed) {
            startError = errorIfFailed;
            dispatch_semaphore_signal(startSem);
        }];

        // Pump runloop while waiting for VM to start
        while (dispatch_semaphore_wait(startSem, DISPATCH_TIME_NOW) != 0) {
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
        }

        if (startError) {
            fprintf(stderr, "Error: Failed to start VM: %s\n", [[startError localizedDescription] UTF8String]);
            return 1;
        }

        // Start TCP port forwarders for any mapped container ports
        for (NSString *pSpec in portSpecs) {
            NSArray *parts = [pSpec componentsSeparatedByString:@":"];
            NSString *hostIp = @"0.0.0.0";
            int hostPort = 0;
            int vmPort = 0;
            if ([parts count] == 2) {
                hostPort = [parts[0] intValue];
                vmPort = [parts[1] intValue];
            } else if ([parts count] == 3) {
                hostIp = parts[0];
                hostPort = [parts[1] intValue];
                vmPort = [parts[2] intValue];
            }
            if (hostPort > 0 && vmPort > 0) {
                struct PortForwardSpec *spec = malloc(sizeof(struct PortForwardSpec));
                spec->host_port = hostPort;
                spec->vm_port = vmPort;
                memset(spec->host_ip, 0, sizeof(spec->host_ip));
                memset(spec->vm_ip, 0, sizeof(spec->vm_ip));
                strncpy(spec->host_ip, [hostIp UTF8String], sizeof(spec->host_ip) - 1);
                strncpy(spec->vm_ip, "192.168.64.2", sizeof(spec->vm_ip) - 1);

                pthread_t lt;
                pthread_attr_t attr;
                pthread_attr_init(&attr);
                pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
                pthread_create(&lt, &attr, port_forward_listener_thread, spec);
                pthread_attr_destroy(&attr);
            }
        }

        // Run until guest powers down
        CFRunLoopRun();

        // Ensure VM is stopped cleanly if not already stopped
        if (vm.canStop) {
            dispatch_semaphore_t stopSem = dispatch_semaphore_create(0);
            [vm stopWithCompletionHandler:^(NSError * _Nullable error) {
                dispatch_semaphore_signal(stopSem);
            }];
            dispatch_semaphore_wait(stopSem, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3.0 * NSEC_PER_SEC)));
        }

        if (isDetach && writeHandle) {
            @try {
                [writeHandle synchronizeFile];
                [writeHandle closeFile];
            } @catch (NSException *e) {}
        }

        // Read container exit code from rootfs
        int exitCode = 0;
        NSString *exitCodePath = [rootfsPath stringByAppendingPathComponent:@"boxr-exitcode"];
        if ([[NSFileManager defaultManager] fileExistsAtPath:exitCodePath]) {
            NSString *content = [NSString stringWithContentsOfFile:exitCodePath encoding:NSUTF8StringEncoding error:nil];
            if (content) {
                exitCode = [content intValue];
            }
            [[NSFileManager defaultManager] removeItemAtPath:exitCodePath error:nil];
        }

        // Clean up runner script
        NSString *runScriptPath = [rootfsPath stringByAppendingPathComponent:@"boxr-run.sh"];
        [[NSFileManager defaultManager] removeItemAtPath:runScriptPath error:nil];

        // Clean up PID file
        if (bundlePath) {
            NSString *pidFile = [bundlePath stringByAppendingPathComponent:@"vm.pid"];
            [[NSFileManager defaultManager] removeItemAtPath:pidFile error:nil];
        }

        return exitCode;
    }
}
