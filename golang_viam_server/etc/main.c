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

void INThandler(int);

int s = 0;

/* Default mtu */
static int imtu = 2048;
static int omtu = 2048;

/* Default FCS option */
static int fcs = 0x01;

/* Default Transmission Window */
static int txwin_size = 1000;

/* Default Max Transmission */
static int max_transmit = 30;

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

int main(int argc, char **argv)
{
    signal(SIGINT, INThandler);
    struct sockaddr_l2 addr = { 0 };
    struct sockaddr_l2 local_addr = { 0 };
    struct l2cap_options opts;
    int status;
    char *message = "hello!";
    char dest[18] = "7B:0A:90:17:4F:FC";

    // allocate a socket
    s = socket(AF_BLUETOOTH, SOCK_SEQPACKET, BTPROTO_L2CAP);
    int level = BT_SECURITY_HIGH;
    int err = setsockopt(s, SOL_BLUETOOTH, BT_SECURITY, &level,
                         sizeof(level));
    if (err == -1) {
        perror("setsockopt1");
        return 1;
    }

    

    // set the connection parameters (who to connect to)
    addr.l2_family = AF_BLUETOOTH;
    addr.l2_psm = htobs(192); // FILLIN
    addr.l2_bdaddr_type = BDADDR_LE_RANDOM;
    str2ba( dest, &addr.l2_bdaddr );

    /* Get default options */
    if (getopts(s, &opts, false) < 0) {
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

    if (setopts(s, &opts) < 0) {
        printf("Can't set L2CAP options: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    if (chan_policy != -1) {
        if (setsockopt(s, SOL_BLUETOOTH, BT_CHANNEL_POLICY,
                &chan_policy, sizeof(chan_policy)) < 0) {
            printf("Can't enable chan policy : %s (%d)",
                            strerror(errno), errno);
            return 1;
        }
    }

    /* Enable SO_LINGER */
    if (linger) {
        struct linger l = { .l_onoff = 1, .l_linger = linger };

        if (setsockopt(s, SOL_SOCKET, SO_LINGER, &l, sizeof(l)) < 0) {
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

    if (setsockopt(s, SOL_L2CAP, L2CAP_LM, &opt, sizeof(opt)) < 0) {
        printf("Can't set L2CAP link mode: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    /* Set receive buffer size */
    if (rcvbuf && setsockopt(s, SOL_SOCKET, SO_RCVBUF,
                        &rcvbuf, sizeof(rcvbuf)) < 0) {
        printf("Can't set socket rcv buf size: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

    socklen_t optlen;
    optlen = sizeof(rcvbuf);
    if (getsockopt(s, SOL_SOCKET, SO_RCVBUF, &rcvbuf, &optlen) < 0) {
        printf("Can't get socket rcv buf size: %s (%d)",
                            strerror(errno), errno);
        return 1;
    }

	// bind socket
    local_addr.l2_family = AF_BLUETOOTH;
    local_addr.l2_bdaddr_type = BDADDR_LE_RANDOM;
    bacpy(&local_addr.l2_bdaddr, BDADDR_ANY);
    local_addr.l2_psm = htobs(192);

    if (bind(s, (struct sockaddr *) &local_addr, sizeof(local_addr)) < 0) {
      perror("bind");
      return 1;
    }

    // Set flow ctl mode.
    int mode = 0x80; // or 0x80 if L2CAP_MODE_LE_FLOWCTL not defined
    err = setsockopt(s, SOL_BLUETOOTH, BT_MODE, &mode, sizeof(mode));
    if (err == -1) {
        perror("setsockopt");
        return 1;
    }

    // connect to server
    printf("connecting...\n");
    status = connect(s, (struct sockaddr *)&addr, sizeof(addr));

    printf("connected %d %d\n", status, errno);
    if( status != 0 ) {
        perror("uh oh not connected");
        close(s);
        s = 0;
        return 1;
    }

    char buf[2048] = { 0 };
    for (int i = 0; i < 50; i++) {
        printf("reading...\n");
        int readBytes = recv(s, buf, imtu, 0);
        int length = buf[0] | (buf[1] << 8);
        printf("read %d %d %d\n", readBytes, length, errno);
        if (readBytes > 0) {
            printf("really read %.*s\n", length, buf+2);
        }

        printf("sleeping...\n");
        sleep(4);
        printf("slept...\n");

        printf("sending 1...\n");
        status = send(s, "\x06\x00hello!", 8, 0);
        printf("sent %d\n", status);
        if( status <= 0 ) {
            perror("uh oh bad write");
            continue;
        }
    }

    close(s);
    s = 0;

    return 1;
}

void  INThandler(int sig)
{
     if (s != 0) {
        printf("closing socket %d\n", s);
        close(s);
     }
     exit(1);
}
