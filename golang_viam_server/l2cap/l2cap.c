#include "l2cap.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/socket.h>
#include <bluetooth/bluetooth.h>
#include <bluetooth/l2cap.h>
#include <errno.h>
#include <signal.h>

/* Default mtu */
static int imtu = 2048;
static int omtu = 2048;

/* Default FCS option */
static int fcs = 0x01;

/* Default Transmission Window */
static int txwin_size = 1000;

/* Default Max Transmission */
static int max_transmit = 30;

/* Other default settings */
static int rfcmode = 0;
static int central = 1;
static int auth = 1;
static int encr = 1;
static int secure = 1;
static int linger = 1;
static int reliable = 1;
static int rcvbuf = 2048;
static int chan_policy = -1;
static int bdaddr_type = 0;

static int getopts(int sk, struct l2cap_options *opts, bool connected)
{
    socklen_t optlen;
    int err;

    memset(opts, 0, sizeof(*opts));

    if (bdaddr_type == BDADDR_BREDR || rfcmode) {
        optlen = sizeof(*opts);
        return getsockopt(sk, SOL_L2CAP, L2CAP_OPTIONS, opts, &optlen);
    }

    optlen = sizeof(opts->imtu);
    err = getsockopt(sk, SOL_BLUETOOTH, BT_RCVMTU, &opts->imtu, &optlen);
    if (err < 0 || !connected)
        return err;

    optlen = sizeof(opts->omtu);
    return getsockopt(sk, SOL_BLUETOOTH, BT_SNDMTU, &opts->omtu, &optlen);
}

static int bt_mode_to_l2cap_mode(int mode)
{
    switch (mode) {
    case BT_MODE_BASIC:
        return L2CAP_MODE_BASIC;
    case BT_MODE_ERTM:
        return L2CAP_MODE_ERTM;
    case BT_MODE_STREAMING:
        return L2CAP_MODE_STREAMING;
    case BT_MODE_LE_FLOWCTL:
        return 0x80;
    case BT_MODE_EXT_FLOWCTL:
        return L2CAP_MODE_FLOWCTL;
    default:
        return mode;
    }
}

static int setopts(int sk, struct l2cap_options *opts)
{
    if (bdaddr_type == BDADDR_BREDR) {
        opts->mode = bt_mode_to_l2cap_mode(opts->mode);
        return setsockopt(sk, SOL_L2CAP, L2CAP_OPTIONS, opts,
                                sizeof(*opts));
    }

    if (opts->mode) {
        if (setsockopt(sk, SOL_BLUETOOTH, BT_MODE, &opts->mode,
                        sizeof(opts->mode)) < 0) {
            return -errno;
        }
    }

    return setsockopt(sk, SOL_BLUETOOTH, BT_RCVMTU, &opts->imtu,
                            sizeof(opts->imtu));
}

int l2cap_dial(const char *address, unsigned int psm, int *out_s)
{
    struct sockaddr_l2 addr = { 0 };
    struct sockaddr_l2 local_addr = { 0 };
    struct l2cap_options opts;
    int status;

    // Allocate a socket.
    *out_s = socket(AF_BLUETOOTH, SOCK_SEQPACKET, BTPROTO_L2CAP);
    int level = BT_SECURITY_HIGH;
    int err = setsockopt(*out_s, SOL_BLUETOOTH, BT_SECURITY, &level,
                         sizeof(level));
    if (err == -1) {
        perror("setsockopt1");
        return 1;
    }

    // set the connection parameters (who to connect to)
    addr.l2_family = AF_BLUETOOTH;
    addr.l2_psm = htobs(psm);
    addr.l2_bdaddr_type = BDADDR_LE_RANDOM;
    str2ba( address, &addr.l2_bdaddr );

    /* Get default options */
    if (getopts(*out_s, &opts, false) < 0) {
        printf("Can't get default L2CAP options: %s (%d)",
                        strerror(errno), errno);
        return 1;
    }

    /* Set new options */
    opts.omtu = omtu;
    opts.imtu = imtu;
    opts.mode = rfcmode;

    opts.fcs = fcs;
    opts.txwin_size = txwin_size;
    opts.max_tx = max_transmit;

    if (setopts(*out_s, &opts) < 0) {
        printf("Can't set L2CAP options: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    if (chan_policy != -1) {
        if (setsockopt(*out_s, SOL_BLUETOOTH, BT_CHANNEL_POLICY,
                &chan_policy, sizeof(chan_policy)) < 0) {
            printf("Can't enable chan policy : %s (%d)",
                            strerror(errno), errno);
            return 1;
        }
    }

    /* Enable SO_LINGER */
    if (linger) {
        struct linger l = { .l_onoff = 1, .l_linger = linger };

        if (setsockopt(*out_s, SOL_SOCKET, SO_LINGER, &l, sizeof(l)) < 0) {
            printf("Can't enable SO_LINGER: %s (%d)",
                            strerror(errno), errno);
            return 1;
        }
    }

    /* Set link mode */
    int opt = 0;
    if (reliable)
        opt |= L2CAP_LM_RELIABLE;
    if (central)
        opt |= L2CAP_LM_MASTER;
    if (auth)
        opt |= L2CAP_LM_AUTH;
    if (encr)
        opt |= L2CAP_LM_ENCRYPT;
    if (secure)
        opt |= L2CAP_LM_SECURE;

    if (setsockopt(*out_s, SOL_L2CAP, L2CAP_LM, &opt, sizeof(opt)) < 0) {
        printf("Can't set L2CAP link mode: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    /* Set receive buffer size */
    if (rcvbuf && setsockopt(*out_s, SOL_SOCKET, SO_RCVBUF,
                        &rcvbuf, sizeof(rcvbuf)) < 0) {
        printf("Can't set socket rcv buf size: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    socklen_t optlen;
    optlen = sizeof(rcvbuf);
    if (getsockopt(*out_s, SOL_SOCKET, SO_RCVBUF, &rcvbuf, &optlen) < 0) {
        printf("Can't get socket rcv buf size: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    // bind socket
    local_addr.l2_family = AF_BLUETOOTH;
    local_addr.l2_bdaddr_type = BDADDR_LE_RANDOM;
    bacpy(&local_addr.l2_bdaddr, BDADDR_ANY);
    local_addr.l2_psm = htobs(psm);

    if (bind(*out_s, (struct sockaddr *) &local_addr, sizeof(local_addr)) < 0) {
        perror("bind");
        return 1;
    }

    // connect to server
    printf("connecting...\n");
    status = connect(*out_s, (struct sockaddr *)&addr, sizeof(addr));

    printf("connected %d %d\n", status, errno);
    if( status != 0 ) {
        perror("uh oh not connected");
        close(*out_s);
        *out_s = 0;
        return 1;
    }

    printf("DEBUG: l2cap_dial has set the socket number to %d\n", *out_s);
    return 0;
}

int l2cap_write(int s, const char* message) {
    printf("l2cap_write is using the socket number of %d\n", s);
    int status;
    printf("writing message %s ...\n", message);
    // Hardcode "hello!" for now.
    status = send(s, "\x06\x00hello!", 8, 0);
    printf("sent %d\n", status);
    if( status <= 0 ) {
        perror("uh oh bad write");
    }
    return 0;
}

int l2cap_read(int s, const char* out_message) {
    printf("l2cap_read is using the socket number of %d\n", s);
    char buf[256] = { 0 };
    printf("reading...\n");
    int readBytes = recv(s, buf, imtu, 0);
    int length = buf[0] | (buf[1] << 8);
    printf("read %d %d %d\n", readBytes, length, errno);
    if (readBytes > 0) {
        printf("really read %.*s\n", length, buf+2);
    }
    return 0;
}

int l2cap_close(int s) {
    return close(s);
}
