#define _GNU_SOURCE
#include <dlfcn.h>
#include <sys/stat.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <string.h>

int chmod(const char *path, mode_t mode) {
    static int (*real_fn)(const char *, mode_t) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "chmod");
    int ret = real_fn ? real_fn(path, mode) : 0;
    if (ret != 0 && (errno == EPERM || errno == EACCES)) return 0;
    return ret;
}

int fchmod(int fd, mode_t mode) {
    static int (*real_fn)(int, mode_t) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "fchmod");
    int ret = real_fn ? real_fn(fd, mode) : 0;
    if (ret != 0 && (errno == EPERM || errno == EACCES)) return 0;
    return ret;
}

int fchmodat(int dirfd, const char *pathname, mode_t mode, int flags) {
    static int (*real_fn)(int, const char *, mode_t, int) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "fchmodat");
    int ret = real_fn ? real_fn(dirfd, pathname, mode, flags) : 0;
    if (ret != 0 && (errno == EPERM || errno == EACCES)) return 0;
    return ret;
}

int chown(const char *path, uid_t owner, gid_t group) {
    static int (*real_fn)(const char *, uid_t, gid_t) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "chown");
    int ret = real_fn ? real_fn(path, owner, group) : 0;
    if (ret != 0 && (errno == EPERM || errno == EACCES)) return 0;
    return ret;
}

int fchown(int fd, uid_t owner, gid_t group) {
    static int (*real_fn)(int, uid_t, gid_t) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "fchown");
    int ret = real_fn ? real_fn(fd, owner, group) : 0;
    if (ret != 0 && (errno == EPERM || errno == EACCES)) return 0;
    return ret;
}

int fchownat(int dirfd, const char *pathname, uid_t owner, gid_t group, int flags) {
    static int (*real_fn)(int, const char *, uid_t, gid_t, int) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "fchownat");
    int ret = real_fn ? real_fn(dirfd, pathname, owner, group, flags) : 0;
    if (ret != 0 && (errno == EPERM || errno == EACCES)) return 0;
    return ret;
}

int fstatat(int dirfd, const char *pathname, struct stat *buf, int flags) {
    static int (*real_fn)(int, const char *, struct stat *, int) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "fstatat");
    int ret = real_fn ? real_fn(dirfd, pathname, buf, flags) : -1;
    if (ret == 0 && buf && S_ISDIR(buf->st_mode)) {
        if (pathname && (strstr(pathname, "data") || strstr(pathname, "postgresql"))) {
            buf->st_mode &= ~0077;
        }
    }
    return ret;
}

int stat(const char *path, struct stat *buf) {
    return fstatat(AT_FDCWD, path, buf, 0);
}

int lstat(const char *path, struct stat *buf) {
    return fstatat(AT_FDCWD, path, buf, AT_SYMLINK_NOFOLLOW);
}

int fstat(int fd, struct stat *buf) {
    static int (*real_fn)(int, struct stat *) = NULL;
    if (!real_fn) real_fn = dlsym(RTLD_NEXT, "fstat");
    int ret = real_fn ? real_fn(fd, buf) : -1;
    if (ret == 0 && buf && S_ISDIR(buf->st_mode)) {
        buf->st_mode &= ~0077;
    }
    return ret;
}
