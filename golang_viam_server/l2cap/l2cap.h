#ifndef _L2CAP_H
#define _L2CAP_H

int l2cap_dial(const char *address, unsigned int psm, int *out_s);
int l2cap_write(int s, const char* message);
int l2cap_read(int s, const char* out_message);
int l2cap_close(int s);

#endif
