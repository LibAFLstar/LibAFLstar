## Create the patch file (for developers)
```
git diff --no-index ./live ./liveNew/ > live555.patch
```
Replace /./live/ and /./liveNew/ with /
Add this part at the end
```
diff --git a/config.linux b/config.linux
index b4021ef..b12ff16 100644
--- a/config.linux
+++ b/config.linux
@@ -1,12 +1,12 @@
 COMPILE_OPTS =		$(INCLUDES) -I/usr/local/include -I. -O2 -DSOCKLEN_T=socklen_t -D_LARGEFILE_SOURCE=1 -D_FILE_OFFSET_BITS=64
 C =			c
-C_COMPILER =		cc
+C_COMPILER =		afl-cc
 C_FLAGS =		$(COMPILE_OPTS) $(CPPFLAGS) $(CFLAGS)
 CPP =			cpp
-CPLUSPLUS_COMPILER =	c++
+CPLUSPLUS_COMPILER =	afl-cc
 CPLUSPLUS_FLAGS =	$(COMPILE_OPTS) -Wall -DBSD=1 $(CPPFLAGS) $(CXXFLAGS)
 OBJ =			o
-LINK =			c++ -o
+LINK =			afl-cc -lstdc++ -o
 LINK_OPTS =		-L. $(LDFLAGS)
 CONSOLE_LINK_OPTS =	$(LINK_OPTS)
 LIBRARY_LINK =		ar cr 
```