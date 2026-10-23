#!/bin/bash
export PATH=/tmp/devicefarm-workspace/execution-696xnj8v/shared-qslg7skm/android-home-upfr1xzy/platform-tools:/tmp/devicefarm-workspace/execution-696xnj8v/shared-qslg7skm/android-home-upfr1xzy/build-tools/34.0.0:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/tmp/devicefarm-workspace/execution-696xnj8v/shared-qslg7skm/android-home-upfr1xzy/bundletool:$PATH
_df_install_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase install ###########'
echo ' '
echo '[DeviceFarm] adb version' 
adb version 
}
_df_pre_test_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase pre_test ###########'
echo ' '
echo '[DeviceFarm] adb devices -l' 
adb devices -l 
}
_df_test_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase test ###########'
echo ' '
echo '[DeviceFarm] bash $DEVICEFARM_TEST_PACKAGE_PATH/ci/mobile/device-farm/run-android.sh' 
bash $DEVICEFARM_TEST_PACKAGE_PATH/ci/mobile/device-farm/run-android.sh 
COMMAND_EXIT_CODE=$?
if [ "$COMMAND_EXIT_CODE" != "0" ]; then
return $COMMAND_EXIT_CODE
fi
}
_df_post_test_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase post_test ###########'
echo ' '
echo '[DeviceFarm] ls -la $DEVICEFARM_LOG_DIR/vieww || true' 
ls -la $DEVICEFARM_LOG_DIR/vieww || true 
}
handle_TERM()
{
echo '[DEVICEFARM] ########### Stop received, exit testspec execution ###########'
pkill -P $$
exit 130
}
handle_EXIT()
{
DEVICEFARM_EXIT_CODE=$?
echo '[DEVICEFARM] ########### Finish executing testspec ###########'
echo ' '
pkill -P $$
echo '[DEVICEFARM] ########### Setting upload permissions ###########'
echo ' '
chmod -R 777 $DEVICEFARM_SCREENSHOT_PATH 2>/dev/null 
echo DEVICEFARM_LOG_DIR: $DEVICEFARM_LOG_DIR >> /tmp/artifacts_description.txt 
chmod -R 777 $DEVICEFARM_LOG_DIR 2>/dev/null
echo ' '
exit $DEVICEFARM_EXIT_CODE
}
trap 'handle_TERM' 2 15
trap 'handle_EXIT' 0
cd /tmp/devicefarm-workspace/execution-696xnj8v/test-package-fjd9og4e
export CHROMEDRIVER_CDNURL=$DEVICEFARM_CHROMEDRIVER_CDNURL
export CHROMELABS_URL=$DEVICEFARM_CHROMEDRIVER_CDNURL
(
echo '[DEVICEFARM] ########### Start executing testspec ###########'
echo ' '
_df_shell_pid=$(exec sh -c 'echo $PPID')
handle_SUBSHELL_EXIT(){
pkill -P $_df_shell_pid
return 0
}
trap 'handle_SUBSHELL_EXIT' 0
_df_install_phase
_df_pre_test_phase
_df_test_phase
TEST_EXIT_CODE=$?
_df_post_test_phase
exit $TEST_EXIT_CODE
) &
SUB_SHELL_PID=$!
wait $SUB_SHELL_PID
SUB_SHELL_EXIT_CODE=$?
echo ' '
exit $SUB_SHELL_EXIT_CODE
