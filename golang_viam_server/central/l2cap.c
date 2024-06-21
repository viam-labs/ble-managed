#include "l2cap.h"

#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <bluetooth/bluetooth.h>
#include <bluetooth/l2cap.h>
#include <errno.h>

int l2cap_dial(const char *address, unsigned int psm, int *out_s) {
    struct sockaddr_l2 addr = { 0 };

    // Allocate a socket.
    int s = socket(AF_BLUETOOTH, SOCK_SEQPACKET, BTPROTO_L2CAP);

    // Set security.
    /*int level = BT_SECURITY_HIGH;*/
    /*int err = setsockopt(s, SOL_BLUETOOTH, BT_SECURITY, &level,*/
                         /*sizeof(level));*/
    /*if (err == -1) {*/
        /*perror("setsockopt");*/
        /*return 1;*/
    /*}*/


    // Set link mode.
    /*int opt = 0;*/
    /*opt |= L2CAP_LM_RELIABLE;*/
    /*opt |= L2CAP_LM_AUTH;*/
    /*opt |= L2CAP_LM_ENCRYPT;*/

    /*err = setsockopt(s, SOL_L2CAP, L2CAP_LM, &opt,*/
                         /*sizeof(opt));*/
    /*if (err == -1) {*/
        /*perror("setsockopt2");*/
        /*return 1;*/
    /*}*/

    // Set flow control.
    /*opt = 0;*/
    /*opt |= BT_MODE_LE_FLOWCTL;*/

    /*err = setsockopt(s, SOL_L2CAP, BT_MODE, &opt,*/
                         /*sizeof(opt));*/
    /*if (err == -1) {*/
        /*// fails cause of https://github.com/torvalds/linux/blob/master/net/bluetooth/Kconfig#L73C8-L73C25?*/
        /*printf("%d\n", errno);*/
        /*return 1;*/
    /*}*/

    // Set the connection parameters (who to connect to).
    str2ba( address, &addr.l2_bdaddr );
    addr.l2_family = AF_BLUETOOTH;
    addr.l2_psm = htobs(psm);
    addr.l2_bdaddr_type = BDADDR_LE_RANDOM;

    // Connect to server
    int e = connect(s, (struct sockaddr *)&addr, sizeof(addr));
    printf("%d\n", errno);

    *out_s = s;
    return e;
}

int l2cap_write(int s, const char* message) {
    return write(s, message, strlen(message));
}

void l2cap_read(int s) {
    char buf[256] = { 0 };

    for (int i = 0; i < 5; i++) {
        sleep(1);
        printf("reading...\n");
        int readBytes = read(s, buf, 256);
        int length = buf[0] | (buf[1] << 8);
        printf("read %d %d %d\n", readBytes, length, errno);
        if (readBytes > 0) {
            printf("really read %.*s\n", length, buf+2);
        }
    }
}

void l2cap_close(int s) {
    close(s);
}
