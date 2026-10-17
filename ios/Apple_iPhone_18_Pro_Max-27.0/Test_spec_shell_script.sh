#!/bin/bash
export PATH=/tmp/devicefarm-workspace/execution-hp4xo346/shared-oesn_ra6/customer-tools-pzuih35o:$PATH
_df_install_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase install ###########'
echo ' '
echo '[DeviceFarm] devicefarm-cli use appium 2 || true' 
devicefarm-cli use appium 2 || true 
echo '[DeviceFarm] devicefarm-cli use python 3.12 || true' 
devicefarm-cli use python 3.12 || true 
echo '[DeviceFarm] xcodebuild -version' 
xcodebuild -version 
echo '[DeviceFarm] appium driver list --installed 2>&1 | head -20 || true' 
appium driver list --installed 2>&1 | head -20 || true 
}
_df_pre_test_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase pre_test ###########'
echo ' '
echo '[DeviceFarm] xcrun devicectl list devices || true' 
xcrun devicectl list devices || true 
echo '[DeviceFarm] xcrun xctrace list devices 2>&1 | head -20 || true' 
xcrun xctrace list devices 2>&1 | head -20 || true 
echo '[DeviceFarm] echo "udid=$DEVICEFARM_DEVICE_UDID os=$DEVICEFARM_DEVICE_OS_VERSION" || true' 
echo "udid=$DEVICEFARM_DEVICE_UDID os=$DEVICEFARM_DEVICE_OS_VERSION" || true 
}
_df_test_phase()
{
echo ' '
echo '[DEVICEFARM] ########### Entering phase test ###########'
echo ' '
echo '[DeviceFarm] bash $DEVICEFARM_TEST_PACKAGE_PATH/ci/mobile/device-farm/run-ios.sh' 
bash $DEVICEFARM_TEST_PACKAGE_PATH/ci/mobile/device-farm/run-ios.sh 
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
devicefarm-cli use xcode 27
cd /tmp/devicefarm-workspace/execution-hp4xo346/test-package-_q8_n49z
xcrun xctrace placeholder-command > /dev/null 2>&1 || true
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
